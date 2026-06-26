//! Group-key derivation, per-message AEAD sealing, and anti-replay (decisions D2/D3).
//!
//! v1 trust model: a single shared mesh secret is stretched with Argon2id into a 32-byte group
//! key; every message is encrypted + authenticated with XChaCha20-Poly1305 (24-byte random
//! nonces, so nonce reuse is a non-issue). Per-peer monotonically increasing sequence numbers are
//! filtered through a [`ReplayWindow`].

use argon2::Argon2;
use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit, OsRng, Payload},
    Key, XChaCha20Poly1305, XNonce,
};

pub const GROUP_KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;
/// Fixed application salt — the input secret is a shared passphrase, not a stored password,
/// so a constant salt is acceptable for deriving the mesh group key.
const KDF_SALT: &[u8] = b"screen-hop/v1/group-key";

/// Stretch the shared mesh secret into a 32-byte group key with Argon2id.
pub fn derive_group_key(passphrase: &str) -> [u8; GROUP_KEY_LEN] {
    let mut key = [0u8; GROUP_KEY_LEN];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), KDF_SALT, &mut key)
        .expect("argon2id with default params and a valid-length output");
    key
}

/// Authenticated, encrypted framing for mesh messages, keyed by the shared group key.
pub struct SecureChannel {
    cipher: XChaCha20Poly1305,
}

impl SecureChannel {
    pub fn new(group_key: &[u8; GROUP_KEY_LEN]) -> Self {
        Self {
            cipher: XChaCha20Poly1305::new(Key::from_slice(group_key)),
        }
    }

    pub fn from_passphrase(passphrase: &str) -> Self {
        Self::new(&derive_group_key(passphrase))
    }

    /// Seal `plaintext` while authenticating `aad`; returns `nonce || ciphertext+tag`.
    pub fn seal(&self, plaintext: &[u8], aad: &[u8]) -> Vec<u8> {
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        let ct = self
            .cipher
            .encrypt(&nonce, Payload { msg: plaintext, aad })
            .expect("xchacha20poly1305 encryption does not fail for in-memory buffers");
        let mut framed = Vec::with_capacity(NONCE_LEN + ct.len());
        framed.extend_from_slice(nonce.as_slice());
        framed.extend_from_slice(&ct);
        framed
    }

    /// Open a `nonce || ciphertext` frame, verifying `aad`. Returns `None` on any failure
    /// (truncated frame, wrong key, tampered ciphertext, or mismatched AAD).
    pub fn open(&self, frame: &[u8], aad: &[u8]) -> Option<Vec<u8>> {
        if frame.len() < NONCE_LEN {
            return None;
        }
        let (nonce, ct) = frame.split_at(NONCE_LEN);
        self.cipher
            .decrypt(XNonce::from_slice(nonce), Payload { msg: ct, aad })
            .ok()
    }
}

/// Sliding-window anti-replay over a single sender's monotonically increasing sequence numbers
/// (the IPsec-style scheme: a high-water mark plus a 64-bit bitmap of recently-seen lower seqs).
#[derive(Debug, Default, Clone)]
pub struct ReplayWindow {
    high: u64,
    bitmap: u64,
    seen_any: bool,
}

impl ReplayWindow {
    pub fn new() -> Self {
        Self::default()
    }

    /// Accept `seq` if it is fresh, updating state. Returns `false` for a replay or a sequence
    /// number older than the 64-wide window.
    pub fn accept(&mut self, seq: u64) -> bool {
        if !self.seen_any {
            self.seen_any = true;
            self.high = seq;
            self.bitmap = 1; // bit 0 == high (seen)
            return true;
        }

        if seq > self.high {
            let shift = seq - self.high;
            self.bitmap = if shift >= 64 { 1 } else { (self.bitmap << shift) | 1 };
            self.high = seq;
            true
        } else {
            let diff = self.high - seq;
            if diff == 0 || diff >= 64 {
                return false; // current high already seen, or older than the window
            }
            let mask = 1u64 << diff;
            if self.bitmap & mask != 0 {
                false
            } else {
                self.bitmap |= mask;
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_is_deterministic_and_secret_sensitive() {
        assert_eq!(derive_group_key("hunter2"), derive_group_key("hunter2"));
        assert_ne!(derive_group_key("hunter2"), derive_group_key("hunter3"));
        assert_eq!(derive_group_key("hunter2").len(), GROUP_KEY_LEN);
    }

    #[test]
    fn seal_then_open_round_trips() {
        let ch = SecureChannel::from_passphrase("mesh-secret");
        let frame = ch.seal(b"hello mesh", b"peerA:7");
        assert_eq!(ch.open(&frame, b"peerA:7").as_deref(), Some(&b"hello mesh"[..]));
    }

    #[test]
    fn two_seals_use_distinct_nonces() {
        let ch = SecureChannel::from_passphrase("mesh-secret");
        assert_ne!(ch.seal(b"x", b""), ch.seal(b"x", b""));
    }

    #[test]
    fn open_rejects_wrong_aad() {
        let ch = SecureChannel::from_passphrase("mesh-secret");
        let frame = ch.seal(b"payload", b"peerA:7");
        assert_eq!(ch.open(&frame, b"peerA:8"), None);
    }

    #[test]
    fn open_rejects_tampered_ciphertext() {
        let ch = SecureChannel::from_passphrase("mesh-secret");
        let mut frame = ch.seal(b"payload", b"");
        let last = frame.len() - 1;
        frame[last] ^= 0xFF;
        assert_eq!(ch.open(&frame, b""), None);
    }

    #[test]
    fn open_rejects_other_key_and_short_frame() {
        let a = SecureChannel::from_passphrase("secret-A");
        let b = SecureChannel::from_passphrase("secret-B");
        let frame = a.seal(b"payload", b"");
        assert_eq!(b.open(&frame, b""), None);
        assert_eq!(a.open(&[0u8; 8], b""), None);
    }

    #[test]
    fn replay_accepts_fresh_rejects_duplicates() {
        let mut w = ReplayWindow::new();
        assert!(w.accept(1));
        assert!(!w.accept(1)); // exact replay
        assert!(w.accept(2));
        assert!(w.accept(3));
        assert!(!w.accept(2)); // replay within window
    }

    #[test]
    fn replay_accepts_out_of_order_within_window() {
        let mut w = ReplayWindow::new();
        assert!(w.accept(10));
        assert!(w.accept(8)); // older but unseen, within window
        assert!(!w.accept(8)); // now a replay
        assert!(w.accept(9));
    }

    #[test]
    fn replay_rejects_too_old() {
        let mut w = ReplayWindow::new();
        assert!(w.accept(100));
        assert!(!w.accept(100 - 64)); // exactly the window edge / beyond
        assert!(!w.accept(1));
    }

    #[test]
    fn replay_handles_large_forward_jump() {
        let mut w = ReplayWindow::new();
        assert!(w.accept(1));
        assert!(w.accept(1_000)); // jump > 64 clears the window
        assert!(!w.accept(1)); // old seq now outside the window
        assert!(w.accept(1_001));
    }
}

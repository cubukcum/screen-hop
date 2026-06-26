//! Per-install peer identity (Ed25519) and TOFU pin store (decision D2).
//!
//! The group key gates who can talk on the mesh; the Ed25519 identity *names* a peer and lets it
//! prove possession of its key during the handshake, so a verified `peer_id` can be trusted. A
//! changed key for a known peer is rejected (trust-on-first-use), catching revocation/MITM
//! within the mesh.

use std::collections::HashMap;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand_core::OsRng;

/// A peer's long-term signing identity.
pub struct PeerIdentity {
    signing: SigningKey,
}

impl PeerIdentity {
    /// Generate a fresh identity from the OS CSPRNG.
    pub fn generate() -> Self {
        Self {
            signing: SigningKey::generate(&mut OsRng),
        }
    }

    /// Reconstruct from persisted secret-key bytes.
    pub fn from_secret_bytes(bytes: &[u8; 32]) -> Self {
        Self {
            signing: SigningKey::from_bytes(bytes),
        }
    }

    /// The 32-byte secret to persist (store via the OS keystore / DPAPI in production).
    pub fn secret_bytes(&self) -> [u8; 32] {
        self.signing.to_bytes()
    }

    pub fn public_bytes(&self) -> [u8; 32] {
        self.signing.verifying_key().to_bytes()
    }

    /// Stable peer id = hex of the public key.
    pub fn peer_id(&self) -> String {
        peer_id_of(&self.public_bytes())
    }

    /// Sign a message (e.g. a handshake challenge nonce).
    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.signing.sign(message).to_bytes()
    }
}

/// Derive the canonical peer id (hex public key) from raw public-key bytes.
pub fn peer_id_of(public_bytes: &[u8; 32]) -> String {
    to_hex(public_bytes)
}

/// Verify `signature` over `message` against `public_bytes`. False on any malformed input.
pub fn verify(public_bytes: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> bool {
    let Ok(vk) = VerifyingKey::from_bytes(public_bytes) else {
        return false;
    };
    vk.verify(message, &Signature::from_bytes(signature)).is_ok()
}

/// Trust-on-first-use store of `peer_id -> public key`.
#[derive(Debug, Default, Clone)]
pub struct PinStore {
    pins: HashMap<String, [u8; 32]>,
}

/// A known peer presented a different key than the one pinned — possible MITM or key rotation.
#[derive(Debug, PartialEq, Eq)]
pub struct PinMismatch;

impl PinStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pin `peer_id` to `public_bytes` on first sight; on later sight require it to match.
    pub fn check_or_pin(&mut self, peer_id: &str, public_bytes: [u8; 32]) -> Result<(), PinMismatch> {
        match self.pins.get(peer_id) {
            Some(existing) if *existing != public_bytes => Err(PinMismatch),
            Some(_) => Ok(()),
            None => {
                self.pins.insert(peer_id.to_owned(), public_bytes);
                Ok(())
            }
        }
    }

    pub fn is_pinned(&self, peer_id: &str) -> bool {
        self.pins.contains_key(peer_id)
    }

    /// Pre-pin a known peer (e.g. loaded from disk).
    pub fn pin(&mut self, peer_id: &str, public_bytes: [u8; 32]) {
        self.pins.insert(peer_id.to_owned(), public_bytes);
    }
}

fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_then_verify_round_trips() {
        let id = PeerIdentity::generate();
        let sig = id.sign(b"challenge");
        assert!(verify(&id.public_bytes(), b"challenge", &sig));
    }

    #[test]
    fn verify_rejects_wrong_message_or_key() {
        let id = PeerIdentity::generate();
        let other = PeerIdentity::generate();
        let sig = id.sign(b"challenge");
        assert!(!verify(&id.public_bytes(), b"different", &sig));
        assert!(!verify(&other.public_bytes(), b"challenge", &sig));
    }

    #[test]
    fn identity_persists_via_secret_bytes() {
        let id = PeerIdentity::generate();
        let restored = PeerIdentity::from_secret_bytes(&id.secret_bytes());
        assert_eq!(id.peer_id(), restored.peer_id());
        assert_eq!(id.public_bytes(), restored.public_bytes());
    }

    #[test]
    fn peer_id_is_hex_of_public_key() {
        let id = PeerIdentity::generate();
        assert_eq!(id.peer_id().len(), 64);
        assert_eq!(id.peer_id(), peer_id_of(&id.public_bytes()));
    }

    #[test]
    fn pin_store_tofu_accepts_first_rejects_changed_key() {
        let mut pins = PinStore::new();
        let key_a = [1u8; 32];
        let key_b = [2u8; 32];
        assert!(pins.check_or_pin("peer", key_a).is_ok()); // first use
        assert!(pins.check_or_pin("peer", key_a).is_ok()); // same key again
        assert_eq!(pins.check_or_pin("peer", key_b), Err(PinMismatch)); // changed -> reject
        assert!(pins.is_pinned("peer"));
    }
}

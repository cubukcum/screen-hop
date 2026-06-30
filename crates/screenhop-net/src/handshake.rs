//! Mutual identity handshake over a group-key-secured stream (decision D2).
//!
//! Both peers run the same symmetric exchange on a fresh connection:
//!   1. send `Hello { peer_id, pubkey, nonce }`; receive the other's `Hello`
//!   2. send `Proof = sign(their nonce)`; receive and verify their `Proof` over our nonce
//!   3. TOFU-pin the verified peer
//!
//! Success binds the connection to a cryptographically verified `peer_id`. The group key already
//! gates who can speak; this proves *which* mesh member is on the other end.

use std::io::{self, Read, Write};

use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};

use crate::crypto::SecureChannel;
use crate::identity::{peer_id_of, verify, PeerIdentity, PinStore};
use crate::transport::{read_frame, write_frame};

const HS_AAD: &[u8] = b"screen-hop/handshake/v1";
const NONCE_LEN: usize = 32;

#[derive(Serialize, Deserialize)]
#[serde(tag = "hs", rename_all = "snake_case")]
enum HsMsg {
    Hello {
        peer_id: String,
        pubkey: Vec<u8>,
        nonce: Vec<u8>,
    },
    Proof {
        signature: Vec<u8>,
    },
}

/// A cryptographically verified remote identity.
#[derive(Debug, Clone)]
pub struct Verified {
    pub peer_id: String,
    pub public_bytes: [u8; 32],
}

#[derive(Debug)]
pub enum HandshakeError {
    Io(io::Error),
    /// A frame failed AEAD open (wrong group key or tampered).
    Decrypt,
    /// Malformed or unexpected handshake message.
    Parse,
    /// The presented public key did not match the claimed `peer_id`.
    BadKey,
    /// The challenge signature did not verify.
    BadProof,
    /// A known peer presented a different key than pinned (possible MITM / rotation).
    PinMismatch,
}

impl From<io::Error> for HandshakeError {
    fn from(e: io::Error) -> Self {
        HandshakeError::Io(e)
    }
}

/// Run the mutual handshake on `stream`, returning the verified remote identity.
pub fn handshake<S: Read + Write>(
    stream: &mut S,
    channel: &SecureChannel,
    me: &PeerIdentity,
    pins: &mut PinStore,
) -> Result<Verified, HandshakeError> {
    // 1. Exchange Hello. Both peers write first; the messages are small enough to fit the socket
    //    buffer, so writing before reading does not deadlock.
    let my_nonce = random_nonce();
    send(
        stream,
        channel,
        &HsMsg::Hello {
            peer_id: me.peer_id(),
            pubkey: me.public_bytes().to_vec(),
            nonce: my_nonce.to_vec(),
        },
    )?;

    let (their_id, their_pubkey, their_nonce) = match recv(stream, channel)? {
        HsMsg::Hello {
            peer_id,
            pubkey,
            nonce,
        } => (peer_id, pubkey, nonce),
        _ => return Err(HandshakeError::Parse),
    };
    let their_pubkey: [u8; 32] = their_pubkey
        .as_slice()
        .try_into()
        .map_err(|_| HandshakeError::BadKey)?;
    if peer_id_of(&their_pubkey) != their_id {
        return Err(HandshakeError::BadKey);
    }

    // 2. Prove possession: sign their nonce; verify their signature over our nonce.
    send(
        stream,
        channel,
        &HsMsg::Proof {
            signature: me.sign(&their_nonce).to_vec(),
        },
    )?;
    let their_sig = match recv(stream, channel)? {
        HsMsg::Proof { signature } => signature,
        _ => return Err(HandshakeError::Parse),
    };
    let their_sig: [u8; 64] = their_sig
        .as_slice()
        .try_into()
        .map_err(|_| HandshakeError::BadProof)?;
    if !verify(&their_pubkey, &my_nonce, &their_sig) {
        return Err(HandshakeError::BadProof);
    }

    // 3. Trust-on-first-use pin.
    pins.check_or_pin(&their_id, their_pubkey)
        .map_err(|_| HandshakeError::PinMismatch)?;

    Ok(Verified {
        peer_id: their_id,
        public_bytes: their_pubkey,
    })
}

fn send<S: Write>(
    stream: &mut S,
    channel: &SecureChannel,
    msg: &HsMsg,
) -> Result<(), HandshakeError> {
    let bytes = serde_json::to_vec(msg).expect("handshake message serializes");
    write_frame(stream, &channel.seal(&bytes, HS_AAD))?;
    Ok(())
}

fn recv<S: Read>(stream: &mut S, channel: &SecureChannel) -> Result<HsMsg, HandshakeError> {
    let frame = read_frame(stream)?;
    let plaintext = channel
        .open(&frame, HS_AAD)
        .ok_or(HandshakeError::Decrypt)?;
    serde_json::from_slice(&plaintext).map_err(|_| HandshakeError::Parse)
}

fn random_nonce() -> [u8; NONCE_LEN] {
    let mut nonce = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce);
    nonce
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    fn channel() -> SecureChannel {
        SecureChannel::from_passphrase("mesh")
    }

    #[test]
    fn mutual_handshake_verifies_both_identities() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let me = PeerIdentity::generate();
            let my_id = me.peer_id();
            let mut pins = PinStore::new();
            let v = handshake(&mut sock, &channel(), &me, &mut pins).unwrap();
            (my_id, v.peer_id)
        });

        let mut client = TcpStream::connect(addr).unwrap();
        let me = PeerIdentity::generate();
        let my_id = me.peer_id();
        let mut pins = PinStore::new();
        let v = handshake(&mut client, &channel(), &me, &mut pins).unwrap();

        let (server_id, server_saw_client) = server.join().unwrap();
        assert_eq!(v.peer_id, server_id); // client verified the server's identity
        assert_eq!(server_saw_client, my_id); // server verified the client's identity
        assert!(pins.is_pinned(&server_id));
    }

    #[test]
    fn handshake_fails_with_mismatched_group_key() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let me = PeerIdentity::generate();
            let mut pins = PinStore::new();
            handshake(
                &mut sock,
                &SecureChannel::from_passphrase("RIGHT"),
                &me,
                &mut pins,
            )
            .is_ok()
        });

        let mut client = TcpStream::connect(addr).unwrap();
        let me = PeerIdentity::generate();
        let mut pins = PinStore::new();
        let result = handshake(
            &mut client,
            &SecureChannel::from_passphrase("WRONG"),
            &me,
            &mut pins,
        );

        assert!(result.is_err());
        assert!(!server.join().unwrap());
    }
}

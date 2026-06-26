//! Secured, length-prefixed framing over any byte stream (TCP in production).
//!
//! Each outbound [`Envelope`] is serialized and sealed with the group-key [`SecureChannel`];
//! each inbound frame is opened, parsed, and anti-replay-checked per sender. The whole envelope
//! (including `from`/`seq`) is inside the AEAD, so a fixed protocol label is sufficient AAD —
//! the v1 threat model is an unpaired LAN host with no group key (a key holder is the trusted
//! single operator).

use std::collections::HashMap;
use std::io::{self, Read, Write};

use crate::crypto::{ReplayWindow, SecureChannel};
use crate::message::{Envelope, Message};

/// Fixed associated-data label binding frames to this protocol/version.
pub const PROTOCOL_AAD: &[u8] = b"screen-hop/mesh/v1";

/// Hard cap on a single frame to bound receive-side allocation.
const MAX_FRAME_LEN: usize = 1 << 20; // 1 MiB

/// Write a `u32` big-endian length prefix followed by `payload`.
pub fn write_frame<W: Write>(w: &mut W, payload: &[u8]) -> io::Result<()> {
    let len = u32::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "frame too large"))?;
    w.write_all(&len.to_be_bytes())?;
    w.write_all(payload)?;
    w.flush()
}

/// Read one length-prefixed frame, rejecting absurd lengths before allocating.
pub fn read_frame<R: Read>(r: &mut R) -> io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame exceeds maximum length",
        ));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

#[derive(Debug)]
pub enum RecvError {
    Io(io::Error),
    /// The frame failed AEAD authentication/decryption (wrong key or tampered).
    Decrypt,
    /// The decrypted bytes were not a valid envelope.
    Parse,
    /// The sender's sequence number was a replay or older than the window.
    Replay,
}

/// A secured connection over a byte stream `S`. Tracks its own outbound sequence counter and a
/// per-sender [`ReplayWindow`] for inbound anti-replay.
pub struct SecureConnection<S> {
    stream: S,
    channel: SecureChannel,
    seq: u64,
    replay: HashMap<String, ReplayWindow>,
}

impl<S: Read + Write> SecureConnection<S> {
    pub fn new(stream: S, channel: SecureChannel) -> Self {
        Self {
            stream,
            channel,
            seq: 0,
            replay: HashMap::new(),
        }
    }

    /// Seal and send `body` as `me`, assigning the next outbound sequence number.
    pub fn send(&mut self, me: &str, body: Message) -> io::Result<()> {
        self.seq += 1;
        let env = Envelope {
            from: me.to_owned(),
            seq: self.seq,
            body,
        };
        self.send_envelope(&env)
    }

    /// Seal and send a fully-formed envelope (lets a caller control `from`/`seq`).
    pub fn send_envelope(&mut self, env: &Envelope) -> io::Result<()> {
        let frame = self.channel.seal(&env.to_bytes(), PROTOCOL_AAD);
        write_frame(&mut self.stream, &frame)
    }

    /// Receive, open, parse, and anti-replay-check the next envelope.
    pub fn recv(&mut self) -> Result<Envelope, RecvError> {
        let frame = read_frame(&mut self.stream).map_err(RecvError::Io)?;
        let plaintext = self
            .channel
            .open(&frame, PROTOCOL_AAD)
            .ok_or(RecvError::Decrypt)?;
        let env = Envelope::from_bytes(&plaintext).ok_or(RecvError::Parse)?;
        let window = self.replay.entry(env.from.clone()).or_default();
        if !window.accept(env.seq) {
            return Err(RecvError::Replay);
        }
        Ok(env)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::SecureChannel;
    use std::io::Cursor;
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    #[test]
    fn frame_round_trips_in_memory() {
        let mut buf = Vec::new();
        write_frame(&mut buf, b"hello").unwrap();
        let mut cur = Cursor::new(buf);
        assert_eq!(read_frame(&mut cur).unwrap(), b"hello");
    }

    #[test]
    fn read_frame_rejects_oversized_length() {
        let mut cur = Cursor::new(u32::MAX.to_be_bytes().to_vec());
        assert!(read_frame(&mut cur).is_err());
    }

    fn loopback() -> (TcpListener, std::net::SocketAddr) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let a = l.local_addr().unwrap();
        (l, a)
    }

    #[test]
    fn secured_envelope_round_trips_over_loopback_tcp() {
        let (listener, addr) = loopback();
        let server = thread::spawn(move || {
            let (sock, _) = listener.accept().unwrap();
            let mut conn = SecureConnection::new(sock, SecureChannel::from_passphrase("secret"));
            conn.recv().map(|e| e.body).unwrap()
        });

        let client = TcpStream::connect(addr).unwrap();
        let mut conn = SecureConnection::new(client, SecureChannel::from_passphrase("secret"));
        conn.send("peerA", Message::Heartbeat { state_version: 7 }).unwrap();

        assert_eq!(server.join().unwrap(), Message::Heartbeat { state_version: 7 });
    }

    #[test]
    fn wrong_passphrase_is_rejected() {
        let (listener, addr) = loopback();
        let server = thread::spawn(move || {
            let (sock, _) = listener.accept().unwrap();
            let mut conn = SecureConnection::new(sock, SecureChannel::from_passphrase("RIGHT"));
            matches!(conn.recv(), Err(RecvError::Decrypt))
        });

        let client = TcpStream::connect(addr).unwrap();
        let mut conn = SecureConnection::new(client, SecureChannel::from_passphrase("WRONG"));
        conn.send("peerA", Message::Heartbeat { state_version: 1 }).unwrap();

        assert!(server.join().unwrap());
    }

    #[test]
    fn replayed_sequence_is_rejected_over_the_wire() {
        let (listener, addr) = loopback();
        let server = thread::spawn(move || {
            let (sock, _) = listener.accept().unwrap();
            let mut conn = SecureConnection::new(sock, SecureChannel::from_passphrase("s"));
            let first = conn.recv().is_ok();
            let second_replayed = matches!(conn.recv(), Err(RecvError::Replay));
            first && second_replayed
        });

        let client = TcpStream::connect(addr).unwrap();
        let mut conn = SecureConnection::new(client, SecureChannel::from_passphrase("s"));
        let env = Envelope {
            from: "peerA".into(),
            seq: 1,
            body: Message::Heartbeat { state_version: 1 },
        };
        conn.send_envelope(&env).unwrap();
        conn.send_envelope(&env).unwrap(); // same seq -> replay

        assert!(server.join().unwrap());
    }
}

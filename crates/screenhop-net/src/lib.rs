//! LAN mesh primitives for screen-hop (milestone M3).
//!
//! - [`crypto`]: group-key derivation (Argon2id), per-message AEAD sealing
//!   (XChaCha20-Poly1305), and a sliding-window anti-replay filter.
//! - [`message`]: the serde wire schema (envelopes + message variants).
//!
//! Transport (TCP) and discovery (mDNS) wrap these; the cryptography and the
//! replay/serialization logic are pure and unit-tested here.

pub mod crypto;
pub mod message;
pub mod transport;

pub use crypto::{derive_group_key, ReplayWindow, SecureChannel, GROUP_KEY_LEN};
pub use message::{Envelope, Message, MonitorId, PeerId};
pub use transport::{read_frame, write_frame, RecvError, SecureConnection, PROTOCOL_AAD};

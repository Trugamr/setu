//! Setu desktop app: receives and plays audio streamed from the headset.
//!
//! [`Receiver`] is the whole app behind one handle. The modules under it:
//!
//! - [`protocol`]: the wire format shared with the headset (`protocol/README.md`).
//! - [`link`]: the UDP conversation with the headset's capture server.
//! - [`playout`]: the jitter buffer and rate controller between network and device.
//! - [`output`]: the audio device, behind a platform adapter.

pub mod link;
pub mod output;
pub mod playout;
pub mod protocol;
pub mod receiver;

pub use receiver::{Config, Connection, Receiver, Status};

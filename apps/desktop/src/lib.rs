//! Setu desktop app: receives and plays audio streamed from the headset.
//!
//! [`Receiver`] plays the headset's audio behind one handle. The modules under it:
//!
//! - [`protocol`]: the wire format shared with the headset (`protocol/README.md`).
//! - [`link`]: the UDP conversation with the headset's capture server.
//! - [`playout`]: the jitter buffer and rate controller between network and device, which
//!   measures how late datagrams arrive with `jitter`.
//! - [`output`]: the audio device, behind a platform adapter.
//! - `priority`: the scheduling priority of the link's and the output's threads.
//!
//! [`Session`] is the app around a receiver, which the front ends (the window in `main.rs` and
//! `setu-cli`) host:
//!
//! - [`session`]: the settings, the receiver while connected, and the commands that change them.
//! - [`control`]: drives the running app's session from another process.
//! - [`view`]: the wording and meter level a front end shows for a receiver's status.
//! - [`settings`]: what the app remembers between runs.

pub mod control;
mod jitter;
pub mod link;
pub mod output;
pub mod playout;
mod priority;
pub mod protocol;
pub mod receiver;
pub mod session;
pub mod settings;
pub mod view;

pub use receiver::{Config, Connection, Receiver, Status, resolve_headset};
pub use session::{Command, Session, Snapshot};

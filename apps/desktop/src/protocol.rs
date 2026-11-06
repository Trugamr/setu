//! Setu's stream protocol (spec: `protocol/README.md`). Pure byte work with no I/O.
//!
//! [`decode`] reads any message; the receiver only ever sends HELLO, so that is the one encoder.

use std::fmt;

pub const VERSION: u8 = 1;
pub const UDP_PORT: u16 = 27184;
/// The server stops streaming to a receiver that has not sent HELLO for this long.
pub const STALE_RECEIVER_MS: u64 = 3000;

const MAGIC: [u8; 2] = *b"ST";
const HEADER_BYTES: usize = 4;

const HELLO: u8 = 1;
const FORMAT: u8 = 2;
const AUDIO: u8 = 3;
const STATUS_REQUEST: u8 = 4;
const STATUS: u8 = 5;
const STOP: u8 = 6;

/// Sample format of the stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Format {
    pub sample_rate: u32,
    pub channels: u16,
    pub bytes_per_sample: u16,
    pub block_frames: u32,
}

impl Format {
    pub fn frame_bytes(&self) -> usize {
        usize::from(self.channels) * usize::from(self.bytes_per_sample)
    }

    /// Duration of one capture block.
    pub fn block_ms(&self) -> f64 {
        f64::from(self.block_frames) * 1000.0 / f64::from(self.sample_rate)
    }
}

/// One AUDIO datagram. `pcm` borrows from the decoded buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Audio<'a> {
    /// Counts datagrams from 0 for each capture; a gap is a lost datagram.
    pub sequence: u32,
    /// Index of the first frame since the capture began.
    pub frame_position: u64,
    pub pcm: &'a [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receiver {
    pub name: String,
    /// `host:port` as the server sees it.
    pub address: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub block_frames: u32,
    pub receivers: Vec<Receiver>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message<'a> {
    Hello { name: String },
    Format(Format),
    Audio(Audio<'a>),
    StatusRequest,
    Status(Status),
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// Another magic or version: not a datagram for this receiver.
    NotSetu,
    UnknownType(u8),
    Truncated,
    InvalidUtf8,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::NotSetu => write!(f, "not a Setu version {VERSION} message"),
            DecodeError::UnknownType(t) => write!(f, "unknown message type {t}"),
            DecodeError::Truncated => write!(f, "message truncated"),
            DecodeError::InvalidUtf8 => write!(f, "string is not UTF-8"),
        }
    }
}

impl std::error::Error for DecodeError {}

pub fn decode(data: &[u8]) -> Result<Message<'_>, DecodeError> {
    if data.len() < HEADER_BYTES || data[..2] != MAGIC || data[2] != VERSION {
        return Err(DecodeError::NotSetu);
    }
    let mut r = Reader {
        data,
        pos: HEADER_BYTES,
    };
    match data[3] {
        HELLO => Ok(Message::Hello { name: r.string()? }),
        FORMAT => Ok(Message::Format(Format {
            sample_rate: r.u32()?,
            channels: r.u16()?,
            bytes_per_sample: r.u16()?,
            block_frames: r.u32()?,
        })),
        AUDIO => Ok(Message::Audio(Audio {
            sequence: r.u32()?,
            frame_position: r.u64()?,
            pcm: &data[r.pos..],
        })),
        STATUS_REQUEST => Ok(Message::StatusRequest),
        STATUS => {
            let block_frames = r.u32()?;
            let count = r.u16()?;
            let receivers = (0..count)
                .map(|_| {
                    Ok(Receiver {
                        name: r.string()?,
                        address: r.string()?,
                    })
                })
                .collect::<Result<_, DecodeError>>()?;
            Ok(Message::Status(Status {
                block_frames,
                receivers,
            }))
        }
        STOP => Ok(Message::Stop),
        other => Err(DecodeError::UnknownType(other)),
    }
}

/// HELLO, which a receiver sends at least once a second to keep the stream coming.
///
/// # Panics
/// If `name` is longer than 65535 bytes.
pub fn encode_hello(name: &str) -> Vec<u8> {
    let len = u16::try_from(name.len()).expect("receiver name longer than 65535 bytes");
    let mut out = Vec::with_capacity(HEADER_BYTES + 2 + name.len());
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&[VERSION, HELLO]);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(name.as_bytes());
    out
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let bytes = self
            .data
            .get(self.pos..self.pos + N)
            .ok_or(DecodeError::Truncated)?;
        self.pos += N;
        Ok(bytes.try_into().expect("slice has length N"))
    }

    fn u16(&mut self) -> Result<u16, DecodeError> {
        self.take().map(u16::from_le_bytes)
    }

    fn u32(&mut self) -> Result<u32, DecodeError> {
        self.take().map(u32::from_le_bytes)
    }

    fn u64(&mut self) -> Result<u64, DecodeError> {
        self.take().map(u64::from_le_bytes)
    }

    fn string(&mut self) -> Result<String, DecodeError> {
        let len = usize::from(self.u16()?);
        let bytes = self
            .data
            .get(self.pos..self.pos + len)
            .ok_or(DecodeError::Truncated)?;
        self.pos += len;
        String::from_utf8(bytes.to_vec()).map_err(|_| DecodeError::InvalidUtf8)
    }
}

/// Parses a commented-hex fixture from `protocol/fixtures`.
#[cfg(test)]
pub(crate) fn parse_fixture(text: &str) -> Vec<u8> {
    text.lines()
        .map(|line| line.split('#').next().unwrap_or_default())
        .flat_map(str::split_whitespace)
        .map(|b| u8::from_str_radix(b, 16).expect("fixture byte is hex"))
        .collect()
}

/// The bytes of a fixture, e.g. `fixture!("format")`.
#[cfg(test)]
macro_rules! fixture {
    ($name:literal) => {
        $crate::protocol::parse_fixture(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../protocol/fixtures/",
            $name,
            ".hex"
        )))
    };
}

#[cfg(test)]
pub(crate) use fixture;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello() {
        let bytes = fixture!("hello");
        assert_eq!(encode_hello("studio-pc"), bytes);
        assert_eq!(
            decode(&bytes),
            Ok(Message::Hello {
                name: "studio-pc".into()
            })
        );
    }

    #[test]
    fn format() {
        let expected = Format {
            sample_rate: 48000,
            channels: 2,
            bytes_per_sample: 2,
            block_frames: 96,
        };
        assert_eq!(decode(&fixture!("format")), Ok(Message::Format(expected)));
        assert_eq!(expected.frame_bytes(), 4);
        assert_eq!(expected.block_ms(), 2.0);
    }

    #[test]
    fn audio() {
        let bytes = fixture!("audio");
        let pcm = [0x01, 0x00, 0xff, 0xff, 0x00, 0x01, 0x00, 0xff];
        assert_eq!(
            decode(&bytes),
            Ok(Message::Audio(Audio {
                sequence: 7,
                frame_position: 1000,
                pcm: &pcm
            }))
        );
    }

    #[test]
    fn status() {
        let expected = Status {
            block_frames: 96,
            receivers: vec![Receiver {
                name: "studio-pc".into(),
                address: "192.168.1.20:50000".into(),
            }],
        };
        assert_eq!(decode(&fixture!("status")), Ok(Message::Status(expected)));
    }

    #[test]
    fn empty_messages() {
        assert_eq!(
            decode(&fixture!("status-request")),
            Ok(Message::StatusRequest)
        );
        assert_eq!(decode(&fixture!("stop")), Ok(Message::Stop));
    }

    #[test]
    fn foreign_datagrams_are_not_setu() {
        assert_eq!(decode(b"QAHI"), Err(DecodeError::NotSetu));
        assert_eq!(decode(&[b'S', b'T', 2, HELLO]), Err(DecodeError::NotSetu));
        assert_eq!(decode(b"ST"), Err(DecodeError::NotSetu));
        assert_eq!(
            decode(&[b'S', b'T', VERSION, 99]),
            Err(DecodeError::UnknownType(99))
        );
    }

    #[test]
    fn truncated_messages_are_rejected() {
        let bytes = fixture!("status");
        assert_eq!(
            decode(&bytes[..bytes.len() - 1]),
            Err(DecodeError::Truncated)
        );
        assert_eq!(
            decode(&fixture!("format")[..10]),
            Err(DecodeError::Truncated)
        );
    }
}

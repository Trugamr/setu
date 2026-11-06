# Stream protocol

The messages exchanged by the capture server on the headset, the desktop receiver and the agent.
Version 1. The server can also send the audio as [VBAN](#vban), for receivers that are not Setu's.

The [fixtures](fixtures) hold one example of every message as commented hex. The Java module
(`apps/agent/.../protocol/StreamProtocol.java`) and the Rust module (`apps/desktop/src/protocol.rs`)
test against them, so a change here has to land on both sides. `vban-audio` pins the Java VBAN
encoder (`.../protocol/Vban.java`); the desktop app does not read VBAN.

## Framing

All integers are little-endian. Every message starts with a 4-byte header:

| Bytes | Field |
|---|---|
| 0–1 | magic `"ST"` (`53 54`) |
| 2 | version, `1` |
| 3 | type |

A receiver drops datagrams with another magic or version. Strings are a `u16` byte length followed
by UTF-8. An optional VBAN target is a `u8` address length: `0` for none, with nothing after it, or
4 (IPv4) or 16 (IPv6) followed by the address bytes, a `u16 port` and a `string streamName`.

## Messages

| Type | Name | Direction | Payload |
|---|---|---|---|
| 1 | HELLO | receiver → server | `string name` |
| 2 | FORMAT | server → receiver | `u32 sampleRate`, `u16 channels`, `u16 bytesPerSample`, `u32 blockFrames` |
| 3 | AUDIO | server → receiver | `u32 sequence`, `u64 framePosition`, interleaved PCM |
| 4 | STATUS_REQUEST | agent → server | none |
| 5 | STATUS | server → agent | `u32 blockFrames`, `u16 count`, then per receiver `string name`, `string address`; then the VBAN target |
| 6 | STOP | agent → server | none |
| 7 | SET_VBAN | agent → server | VBAN target |

## UDP (Wi-Fi)

The server listens on UDP port 27184.

- **Receiving audio.** A receiver sends HELLO at least once a second. The server answers each
  HELLO with FORMAT and streams AUDIO to the receiver that sent a HELLO most recently. When
  HELLOs stop for 3 seconds the server stops streaming and releases the capture. A receiver the
  server cannot send to is dropped until its next HELLO. FORMAT always precedes the first AUDIO.
- **AUDIO datagrams.** Each one carries at most 340 frames, so a datagram stays under a
  1500-byte MTU. `sequence` counts datagrams from 0 for each capture. `framePosition` is the
  index of the datagram's first frame since the capture began. A gap in `sequence` is a lost
  datagram; a jump in `framePosition` at the same time tells the receiver how much audio it lost.
- **STATUS_REQUEST, STOP and SET_VBAN.** The server accepts these only from a loopback address,
  which means the agent on the same headset. STATUS lists the receiver currently being streamed
  to, or none, and the VBAN target. STOP ends the server process. SET_VBAN replaces the VBAN
  target; none turns VBAN off.

## VBAN

VBAN is VB-Audio's protocol for audio over UDP, which Voicemeeter and other VBAN receivers play
([specification](https://vb-audio.com/Voicemeeter/VBANProtocol_Specifications.pdf), revision 13).
It has no handshake: the sender pushes to an address it was given, and a receiver picks a stream
by the sender's IP address and the stream name.

While a VBAN target is set, the server captures and sends every block to it from its UDP port, in
packets of at most 256 frames. Each packet is a 28-byte little-endian header and interleaved PCM:

| Bytes | Field |
|---|---|
| 0–3 | `"VBAN"` |
| 4 | sample rate index (3 = 48000 Hz) in the low 5 bits; sub-protocol 0 (audio) in the top 3 |
| 5 | frames in the packet − 1 |
| 6 | channels − 1 |
| 7 | sample type, `1` = 16-bit integer, with codec 0 (PCM) in the top 4 bits |
| 8–23 | stream name, ASCII, zero-padded |
| 24–27 | `u32` packet counter, from 0 for each capture |

A Setu receiver and a VBAN target can be served at the same time from one capture.

## TCP (USB, via `adb forward`)

The server sends one FORMAT message, then PCM with no further framing until the connection
closes.

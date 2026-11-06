# Setu

Setu streams a Meta Quest 3's audio to a PC with as little delay as the headset allows. An
earlier prototype measured about 20 ms behind the headset's own speakers over USB and 25–35 ms
over Wi-Fi (estimates, see [Measurements](#measurements)). The headset keeps playing while it
streams. Setu means "bridge"; audio is the first thing it carries.

**Status: work in progress.** The capture server and the protocol work. The agent's start flow,
the desktop app and the installers are being built, so parts of "How it works" describe the
design rather than finished code.

## How it works

The low-latency capture path needs a process running as the adb shell user, which only adbd can
start. Setu sets that up once over USB, after which the headset can start it on its own.

```
 Quest 3                                                PC
 ┌──────────────────────────────────┐                   ┌─────────────────────────┐
 │ Agent app (you open it, or it    │                   │ Desktop app             │
 │ starts after boot)               │                   │  discovers the headset  │
 │  turns on wireless debugging,    │                   │  sends HELLO every 1 s  │
 │  connects to its own adbd with   │                   │  jitter buffer → output │
 │  its own key, launches ──────┐   │                   └────────────▲────────────┘
 │                              ▼   │     UDP 27184                  │
 │  capture server (shell user) ────┼────────────────────────────────┘
 └──────────────────────────────────┘
```

| Piece | What it is |
|---|---|
| Agent (`apps/agent`) | Android app, Kotlin and Compose. Starts the capture server and shows its status and settings. |
| Capture server (`apps/agent/.../server`) | Java, shipped inside the agent's APK, run with `app_process` using the APK as its classpath. Captures playback and serves receivers. |
| Desktop app (`apps/desktop`) | Rust. Finds the headset, receives the stream and plays it. Windows first, then Linux. |
| Installers (planned) | A PC installer inside the desktop app, and a browser installer over WebUSB. Both only install the APK and run the setup routine it contains, so they cannot drift apart. |
| Protocol (`protocol`) | The wire format, with byte fixtures that the Java and Rust sides both test against. |

### One-time setup (USB)

1. Install the agent and grant it `WRITE_SECURE_SETTINGS` (`pm grant`), so it can turn on
   wireless debugging.
2. Set `adb_allowed_connection_time` to `0`, so an authorized key never expires (the default is
   7 days).
3. Authorize the agent's own adb key: `adb tcpip 5555`, the agent connects to
   `127.0.0.1:5555`, and the user ticks "Always allow" on the headset's prompt. Then `adb usb`.
   An approved key is accepted over USB, legacy TCP and wireless debugging alike.

### Each session

The agent sets `adb_wifi_enabled = 1` and finds adbd's TLS port over mDNS
(`_adb-tls-connect._tcp`). It then connects with its key and launches the server detached
(`setsid nohup`), so the server keeps running after wireless debugging is turned off again.
After that, any PC on the network can receive audio; PCs need no adb.

Limits found on Horizon OS:

- **One tap per boot.** Trusted Wi-Fi networks are cleared at every boot, so the first start
  after a boot shows Meta's "Allow wireless debugging" dialog. Horizon OS writes system XML as
  text (`persist.sys.binary_xml=0`); AOSP's keystore reader then treats the whitespace after
  `<keyStore>` as a tag and skips every entry. Authorized adb keys survive because they are
  reloaded from `adb_keys`. Within a boot, Wi-Fi drops and sleep recover silently.
- **Not at boot by itself.** Android tries to restore wireless debugging before Wi-Fi is up and
  gives up, so the agent has to turn it on after Wi-Fi connects.
- **Unknown keys over TLS are rejected without a prompt.** Only legacy TCP (`adb tcpip`) shows
  the "Allow debugging?" prompt, which is why setup uses it once.
- The adb shell holds `MANAGE_DEBUGGING`, so it can trust a network with no dialog
  (`IAdbManager.allowWirelessDebugging`, the server's `adb-wifi-trust` command).
- The Quest Link PC service can stop the "Allow USB debugging?" prompt from appearing; stop it
  while setting up. Toggling Developer Mode in the phone app clears USB authorizations.

## What we learned about Horizon OS audio

- App audio goes through render-only policy mixes to `AUDIO_DEVICE_OUT_BUS` devices, into Meta's
  own audio engine (`media.has`, inside `audioserver`), which mixes in 4 ms blocks.
- Android gives each stream one primary mix and adds every `LOOP_BACK_RENDER` mix as a secondary
  output. Quest's bus mixes are primary, so a `LOOP_BACK` capture loses and records silence,
  while a `LOOP_BACK_RENDER` capture coexists with normal playback. This is why scrcpy's default
  audio source is silent on Quest and `--audio-source=playback --audio-dup` works.
- **The capture block size equals the AudioRecord buffer.** `AudioPolicy.createAudioRecordSink()`
  asks for 1920 frames, so capture runs in 40 ms blocks. Building the AudioRecord directly
  (REMOTE_SUBMIX preset, `addr=<registration>` and `fixedVolume` tags) with a 96-frame buffer
  gives 2 ms blocks. 48 frames is too small: delivery turns into irregular clumps. The sample
  rate does not change the period.
- The VOICE_COMMUNICATION capture preset gives a sink that never receives audio; REMOTE_SUBMIX
  works.
- Capture stops while the headset sleeps. For testing off-head:
  `am broadcast -a com.oculus.vrpowermanager.prox_close`, undone with
  `am broadcast -a com.oculus.vrpowermanager.automation_disable`.
- A shell process can take `WIFI_MODE_FULL_LOW_LATENCY` locks, but Android honors them only for
  a foreground app, and they did not reduce jitter.

### Measurements

Headset side, from a click's presentation time to it being readable by the server (`probe`,
6 clicks each, other audio paused):

| Capture block | Presented → readable |
|---|---|
| 1920 frames (40 ms, Android's default) | 24.0 ms |
| 384 frames (8 ms) | 8.0 ms |
| 192 frames (4 ms) | 7.0 ms |
| 96 frames (2 ms) | 6.3 ms |

Every block size from 96 to 3840 frames arrived on a steady clock (`bursts`; e.g. 96 frames
every 1.9–2.1 ms). Arrival jitter at the PC, as each block's deviation from its nominal spacing:

| Transport | Typical | Worst seen |
|---|---|---|
| USB (adb forward, TCP) | 1–2 ms | 5.6 ms |
| Wi-Fi 6 at −42 dBm, PC wired (UDP) | 6–20 ms | 38.7 ms |

The end-to-end figures at the top add the measured buffer levels to the headset-side delay; true
speaker-to-ear latency has not been measured. Virtual audio mixers on the PC can add delay of
their own, so play to a hardware output for the lowest latency.

### Other approaches and why they fell short

| Approach | Result |
|---|---|
| scrcpy with `--audio-source=playback --audio-dup` | Works, about 80–100 ms behind: 40 ms capture blocks plus scrcpy's buffering. |
| Meta casting | Users report seconds of audio delay. |
| Bluetooth to the PC | 150 ms or more. |
| An ordinary app with MediaProjection | Needs consent each session; whether it gets small capture blocks is untested. |
| A 3.5 mm cable to line-in | Lowest latency, and the fallback. |

## Receiver design notes

These come from the prototype receiver and guide the desktop app.

- **The jitter buffer controls its minimum, not its average.** Block arrivals make the buffer
  level a sawtooth, and latency is set by its bottom. Playback rate is nudged (±0.5 %, linear
  interpolation) so the minimum over each 0.5 s window settles at the margin.
- **Deadband of min(block, output period).** The measured minimum moves by up to that much as
  arrivals drift against the output's pull grid; without the deadband the controller oscillates.
- **Margin** = configured margin + min(block, period). It grows 2 ms per underrun and decays
  0.5 ms after each 15 s without one. Defaults: 1.5 ms over USB, 4 ms over Wi-Fi.
- **After an underrun, refill to one block plus the margin**, not a fixed amount.
- **UDP over Wi-Fi.** Each datagram carries its frame position, so a lost datagram becomes
  silence of the right length instead of a retransmit stall. The receiver sends HELLO and the
  server answers, which also passes desktop firewalls as reply traffic.

## Repository layout

| Path | What |
|---|---|
| `apps/agent` | The agent app; `src/main/java/dev/trugamr/setu/server` is the capture server and `.../protocol` the shared protocol module. |
| `apps/desktop` | The desktop app, a member of the root Cargo workspace. |
| `protocol` | Protocol spec ([protocol/README.md](protocol/README.md)) and byte fixtures. |

## Build

Needs [Task](https://taskfile.dev), JDK 21, the Android SDK (`ANDROID_HOME`, default
`~/Android/Sdk`) and Rust. The Taskfile targets WSL: the agent builds with Linux tools, while
the desktop app and adb use their Windows binaries (see the variables at the top of
`Taskfile.yml`).

```bash
task build                    # agent APK and desktop app
task test                     # JVM and Rust tests
task agent:install            # install the agent on a USB-connected headset
task agent:server -- probe 6  # run the server's latency probe from the installed APK
task desktop:run
```

Server commands (`task agent:server -- <command>`): `udp [port] [blockFrames]`,
`adb [blockFrames]`, `probe [clicks] [blockFrames]`, `bursts [seconds] [blockFrames]`,
`adb-wifi-trust <bssid>`, `adb-revoke-keys`.

## Roadmap

1. Agent: home and settings, start and stop the server, show its status.
2. Desktop app: receiver (protocol, jitter buffer, WASAPI output), tray.
3. Installers: setup routine in the APK, then the PC installer, then the browser installer.
4. Discovery and receiver approval, start at login, a Linux backend.
5. VBAN output, so any VBAN receiver can listen without the desktop app.

Open questions:

- Is browser audio ahead on the PC? Browsers play through a deep buffer that the capture path
  does not show.
- Approving a waiting adb key from the shell (`IAdbManager.allowDebugging`) would remove the
  tap during setup; untested.
- Measure true end-to-end latency by clock-syncing headset and PC over the stream.

## Credits

- Playback capture and the shell-process workarounds are adapted from
  [scrcpy](https://github.com/Genymobile/scrcpy) (Apache-2.0).
- The wireless-debugging approach follows [Shizuku](https://github.com/RikkaApps/Shizuku)
  (Apache-2.0).
- AOSP sources consulted: `AdbDebuggingManager.java` and `AdbService.java` in
  `frameworks/base`, `daemon/auth.cpp` in `packages/modules/adb`, and `adbd_auth.cpp` in
  `frameworks/native`.

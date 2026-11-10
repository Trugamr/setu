# Glossary

The words Setu's code and docs use for its parts.

**Headset**: the Meta Quest whose audio Setu plays.

**Agent**: the Android app installed on the headset. It sets up and starts the capture server.

**Installer**: the browser page (`apps/web`) that installs the agent over WebUSB and runs its
setup, as `task agent:setup` does with adb.

**Capture server**: the process on the headset that captures playback and streams it to receivers.
It runs as the adb shell user, never inside the agent's process.

**Receiver**: on the PC, what plays one headset's stream: the link, the playout and the output
behind one handle. Dropping it stops playback.

**Link**: the receiver's UDP conversation with the capture server: HELLO out, format and audio in.

**Playout**: the jitter buffer and rate controller between the link and the output.

**Output**: the PC's audio device, opened in shared or exclusive mode.

**Session**: the desktop app around a receiver: the settings, the receiver while connected, and
why the last connect failed. Changed only by commands; read through a snapshot.

**Command**: one change to a session: connect, disconnect, choose the device, set exclusive mode.

**Snapshot**: everything a front end shows about a session at one moment.

**Host**: the front end that owns the running session: the window, or `setu-cli play`.

**Control port**: TCP 127.0.0.1:27185, where the host takes requests (a command or `status`)
from `setu-cli`.

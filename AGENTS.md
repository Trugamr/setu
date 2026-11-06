# AGENTS.md

Setu streams Meta Quest audio to a PC at the lowest latency the headset allows. Read
[README.md](README.md) first: it covers the design, how the headset side starts without a cable,
and what is known about Horizon OS audio. Read its "What we learned" section before changing
anything in the capture path.

## Layout

- `apps/agent`: Android app, Gradle project of its own (`dev.trugamr.setu`). Kotlin + Compose UI.
  `src/main/java/dev/trugamr/setu/server` is the capture server in Java; it runs as the adb shell
  user through `app_process` with the APK as classpath, never inside the app process.
  `src/main/java/dev/trugamr/setu/protocol` is the stream protocol, shared by the server and the
  app.
- `apps/desktop`: Rust desktop app, member of the root Cargo workspace.
- `protocol`: the stream protocol spec and its byte fixtures. Both the Java and the Rust protocol
  modules test against the fixtures; change the spec, the fixtures and both modules together.

## Commands

All from WSL via `task` (see `Taskfile.yml`): `task build`, `task test`, `task agent:build`,
`task agent:test`, `task agent:install`, `task agent:server -- <args>`, `task desktop:build`,
`task desktop:run`, `task desktop:test`.

Agent tests are plain JVM tests (`src/test`): keep the logic they cover free of `android.*` and
put the platform behind an interface with an Android adapter, as `Capture` and `Transport` do.

## Environment

- The Taskfile runs in WSL. The desktop app builds with the Windows Rust toolchain
  (`cargo.exe`, output under `%USERPROFILE%\.cache\setu\target`), and the headset is reached
  with Windows `adb.exe`, since WSL has no USB access.
- WSL may have no IPv6 route; JVM tools get `-Djava.net.preferIPv4Stack=true` from the Taskfile.
- The Quest Link PC service (`OVRService`) can stop the headset's "Allow USB debugging?" prompt
  from appearing.

## Working on the headset

- Capture needs the headset awake. Off-head testing:
  `am broadcast -a com.oculus.vrpowermanager.prox_close`; always undo with
  `am broadcast -a com.oculus.vrpowermanager.automation_disable`.
- `probe` detects clicks by amplitude, so other audio must be paused while it runs.
- Don't run headset tests while someone is listening: a UDP server streams to whichever PC said
  hello last, and starting a server stops the running one.

# AGENTS.md

Setu streams Meta Quest audio to a PC. Read the README's "What we learned about Horizon OS audio"
before changing the capture path.

- Build, test and run through `task` (`task --list`). Under WSL it routes to Windows `cargo.exe`
  and `adb.exe` (WSL has no USB) and gives the JVM `-Djava.net.preferIPv4Stack=true`.
- The capture server (`apps/agent/.../server`) runs as the adb shell user via `app_process`,
  never inside the app process.
- Agent tests are plain JVM tests: keep tested logic free of `android.*`, with the platform behind
  an interface and an Android adapter, as `Capture` and `Transport` do.
- The protocol has a Java side (agent) and a Rust side (desktop) pinned by `protocol/fixtures`;
  change the spec, the fixtures and both sides together.

## On the headset

- Capture needs the headset awake. Off-head: `am broadcast -a com.oculus.vrpowermanager.prox_close`;
  undo with `am broadcast -a com.oculus.vrpowermanager.automation_disable`.
- `probe` detects clicks by amplitude: pause other audio first.
- Starting a server stops the running one, so check nobody is listening.
- If the "Allow USB debugging?" prompt never appears, stop the Quest Link PC service
  (`OVRService`).

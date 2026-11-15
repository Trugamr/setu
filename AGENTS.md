# AGENTS.md

Setu streams Meta Quest audio to a PC. Read the README's "What we learned about Horizon OS audio"
before changing the capture path.

- Build, test and run through `task` (`task --list`). Under WSL it routes to Windows `cargo.exe`
  and `adb.exe` (WSL has no USB) and gives the JVM `-Djava.net.preferIPv4Stack=true`.
- CI (`.github/workflows`) can't use the Taskfile, which is WSL-only, so it calls `gradlew`, `pnpm`
  and `cargo` directly. When a task's command changes, change the workflows to match.
- Release builds of the agent run R8. Code reached only by reflection or through `app_process`
  needs a keep rule in `proguard-rules.pro`.
- The capture server (`apps/agent/.../server`) runs as the adb shell user via `app_process`,
  never inside the app process.
- Agent tests are plain JVM tests: keep tested logic free of `android.*`, with the platform behind
  an interface and an Android adapter, as `Capture` and `Transport` do.
- The protocol has a Java side (agent) and a Rust side (desktop) pinned by `protocol/fixtures`;
  change the spec, the fixtures and both sides together.
- Desktop front ends change the app only through `Session` commands. A new action becomes a
  `Command`, so the window and `setu-cli` (through `control`) get it together.
- Setup lives in the APK (`server/Setup.java`, run as `Server setup`); the browser installer
  (`apps/web`) and `task agent:setup` only install, run it, then `adb tcpip 5555`. Change setup
  there, not in an installer.
- To see the agent's screen without a headset, `task agent:screenshots` renders its previews to
  `apps/agent/src/screenshotTestDebug/reference`. Add a `@PreviewTest` for a new state.
- To check the running desktop app, drive it with `task desktop:cli -- status` (or `connect`,
  `device`, `exclusive`, `disconnect`) instead of clicking the window.

## On the headset

- Capture needs the headset awake. Off-head: `am broadcast -a com.oculus.vrpowermanager.prox_close`;
  undo with `am broadcast -a com.oculus.vrpowermanager.automation_disable`.
- `probe` detects clicks by amplitude: pause other audio first.
- Starting a server stops the running one, so check nobody is listening.
- If the "Allow USB debugging?" prompt never appears, stop the Quest Link PC service
  (`OVRService`).

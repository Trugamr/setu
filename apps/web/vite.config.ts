import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";

/** The agent's debug APK, as `task agent:build` leaves it. */
const AGENT_APK = fileURLToPath(new URL("../agent/build/outputs/apk/debug/setu-agent-debug.apk", import.meta.url));
const APK_NAME = "setu-agent.apk";

/**
 * Serves the agent's APK next to the page, read from the agent's build each time, so the page
 * always installs the latest build and never a stale copy.
 */
function agentApk(): Plugin {
  return {
    name: "setu-agent-apk",
    configureServer(server) {
      server.middlewares.use(`/${APK_NAME}`, (_req, res) => {
        let apk: Buffer;
        try {
          apk = readFileSync(AGENT_APK);
        } catch {
          res.statusCode = 404;
          res.end("Build the agent first: task agent:build");
          return;
        }
        res.setHeader("Content-Type", "application/vnd.android.package-archive");
        res.setHeader("Cache-Control", "no-store");
        res.end(apk);
      });
    },
    generateBundle() {
      let source: Buffer;
      try {
        source = readFileSync(AGENT_APK);
      } catch {
        this.error(`${AGENT_APK} is missing. Build the agent first: task agent:build`);
      }
      this.emitFile({ type: "asset", fileName: APK_NAME, source });
    },
  };
}

export default defineConfig({
  // Relative, so the built page works from any path, such as a GitHub Pages project site.
  base: "./",
  // The brand's SVGs live in design/brand, outside this app.
  server: { fs: { allow: ["../.."] } },
  plugins: [react(), tailwindcss(), agentApk()],
});

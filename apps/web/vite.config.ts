import { defineConfig } from "vite";

export default defineConfig({
  // Relative, so the built page works from any path, such as a GitHub Pages project site.
  base: "./",
  // The brand's SVGs live in design/brand, outside this app.
  server: { fs: { allow: ["../.."] } },
});

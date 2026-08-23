import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// Built output is committed into the Python package at dataworm/webapp/dist so
// end users need zero Node tooling: server.py serves that directory for GET "/"
// (with the bearer token injected) plus its /assets/* files.
export default defineConfig({
  plugins: [solid()],
  base: "./",
  build: {
    outDir: "../dataworm/webapp/dist",
    emptyOutDir: true,
    target: "es2022",
  },
});

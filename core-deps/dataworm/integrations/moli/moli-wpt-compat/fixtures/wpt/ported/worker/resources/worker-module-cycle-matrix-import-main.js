import { entry } from "./worker-module-cycle-matrix-import-entry.js";

postMessage({ unexpected: entry });

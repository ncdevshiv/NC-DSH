import "./module_pending_star_cycle_b.mjs";
import { readMissingLater } from "./module_pending_star_cycle_a.mjs";

await readMissingLater();

window.modulePendingStarMissingExportUnexpected = true;

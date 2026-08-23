import { missingValue } from "/assets/module-cycle-missing-export-b.mjs";

export const readyFromA = "a";

window.moduleCycleMissingExportOrder.push(`a:${String(missingValue)}`);

import { missingValue } from "./module_pending_star_cycle_b.mjs";

export async function readMissingLater() {
  await 0;
  return missingValue;
}

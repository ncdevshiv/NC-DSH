import { value } from "/assets/module-runtime-helper-shadowing-source.mjs";

window.moduleRuntimeHelperShadowOrder.push("module");

const __moliModuleRuntime = null;

window.moduleRuntimeHelperShadowValue = value;
window.moduleRuntimeHelperShadowLexicalType = typeof __moliModuleRuntime;
window.moduleRuntimeHelperShadowFinalOrder =
  window.moduleRuntimeHelperShadowOrder.join(",");

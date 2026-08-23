import { first, second } from "docwrite:fixture";

window.documentWriteExternalImportMapOrder.push(`module:${first}:${second}`);
window.documentWriteExternalImportMapResult =
  window.documentWriteExternalImportMapOrder.join(",");

export {};

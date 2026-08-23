import { leaf } from "./worker-module-tla-leaf.js";

self.__httpTlaOrder.push("middle-start");
await Promise.resolve().then(function () {
  self.__httpTlaOrder.push("middle-after");
});
export const middle = leaf + "|middle-start|middle-after";

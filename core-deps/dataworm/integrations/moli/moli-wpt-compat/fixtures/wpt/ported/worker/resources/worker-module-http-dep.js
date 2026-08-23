export const answer = 42;
export const suffix = "dep";
export const depMetaUrl = import.meta.url;
export const depResolvedSelf = import.meta.resolve("./worker-module-http-dep.js");
export const depResolvedDotSegment = import.meta.resolve("./nested/../worker-module-http-dep.js");
export default "default-value";
export function double(value) {
  return value * 2;
}
export class Box {
  constructor(value) {
    this.value = value;
  }
}

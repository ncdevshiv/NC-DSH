import * as dep from "./worker-module-live-dep.js";
import * as forwarded from "./worker-module-live-reexport.js";

const depDescriptor = Object.getOwnPropertyDescriptor(dep, "counter");
const forwardedDescriptor = Object.getOwnPropertyDescriptor(forwarded, "counter");
const before = dep.counter;
const bumpReturn = dep.bump();
const afterBump = dep.counter;
dep.setCounter(9);
const forwardedAfterSet = forwarded.counter;
const forwardedBumpReturn = forwarded.bump();
const afterForwardedBump = dep.counter;

postMessage({
  depDescriptorValue: depDescriptor.value,
  forwardedDescriptorValue: forwardedDescriptor.value,
  depDescriptorEnumerable: depDescriptor.enumerable,
  forwardedDescriptorEnumerable: forwardedDescriptor.enumerable,
  depDescriptorConfigurable: depDescriptor.configurable,
  forwardedDescriptorConfigurable: forwardedDescriptor.configurable,
  before,
  bumpReturn,
  afterBump,
  forwardedAfterSet,
  forwardedBumpReturn,
  afterForwardedBump,
  importScriptsType: typeof importScripts,
});

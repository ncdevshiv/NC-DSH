import current, { counter, bump, setCounter } from "./worker-module-live-import-dep.js";

function readCounter() {
  return counter;
}

const before = counter;
const defaultBefore = current;
const bumpReturn = bump();
const afterBump = readCounter();
setCounter(9);
const afterSet = counter;
const defaultAfterSet = current;
const secondBump = bump();
const afterSecondBump = readCounter();

postMessage({
  before,
  defaultBefore,
  bumpReturn,
  afterBump,
  afterSet,
  defaultAfterSet,
  secondBump,
  afterSecondBump,
  importScriptsType: typeof importScripts,
});

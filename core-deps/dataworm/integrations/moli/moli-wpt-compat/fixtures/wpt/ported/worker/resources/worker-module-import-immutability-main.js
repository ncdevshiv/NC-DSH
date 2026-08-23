import current, { counter, setCounter } from "./worker-module-import-immutability-dep.js";

function assignmentName(callback) {
  try {
    callback();
    return "none";
  } catch (error) {
    return error && error.name;
  }
}

const namedAssignment = assignmentName(function () {
  counter = 7;
});
const defaultAssignment = assignmentName(function () {
  current = 8;
});
setCounter(5);

postMessage({
  namedAssignment,
  defaultAssignment,
  counter,
  current,
  importScriptsType: typeof importScripts,
});

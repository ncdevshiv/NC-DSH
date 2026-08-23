globalThis.workerImportedValue = "loaded";
globalThis.workerImportedEcho = function (value) {
  return "imported:" + value;
};

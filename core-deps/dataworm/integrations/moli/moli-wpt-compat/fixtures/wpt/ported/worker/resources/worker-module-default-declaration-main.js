import defaultFunction, { localFunctionResult } from "./worker-module-default-function-dep.js";
import DefaultClass, { localClassResult } from "./worker-module-default-class-dep.js";
import AnonymousDefaultClass from "./worker-module-default-anonymous-class-dep.js";
import defaultAsyncFunction, { localAsyncFunctionResult } from "./worker-module-default-async-function-dep.js";
import defaultGenerator, { localGeneratorResult } from "./worker-module-default-generator-dep.js";
import defaultAsyncGenerator, { localAsyncGeneratorResult } from "./worker-module-default-async-generator-dep.js";

async function collectAsync(iterable) {
  const values = [];
  for await (const value of iterable) {
    values.push(value);
  }
  return values.join("|");
}

try {
  postMessage({
    importedFunctionResult: defaultFunction("imported"),
    localFunctionResult,
    importedClassResult: new DefaultClass("imported").value,
    localClassResult,
    anonymousClassResult: new AnonymousDefaultClass("anonymous").value,
    importedAsyncFunctionResult: await defaultAsyncFunction("imported"),
    localAsyncFunctionResult,
    importedGeneratorResult: Array.from(defaultGenerator("imported")).join("|"),
    localGeneratorResult,
    importedAsyncGeneratorResult: await collectAsync(defaultAsyncGenerator("imported")),
    localAsyncGeneratorResult,
  });
} catch (error) {
  postMessage({
    error: String(error && error.message ? error.message : error),
  });
}

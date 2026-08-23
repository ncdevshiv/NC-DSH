import * as one from "./worker-module-http-fragment-dep.js#one";
import * as sameOne from "./worker-module-http-fragment-dep.js#one";
import * as two from "./worker-module-http-fragment-dep.js#two";

try {
  const oneFirst = one.bump();
  const oneSecond = sameOne.bump();
  const twoFirst = two.bump();

  postMessage({
    oneUrl: one.moduleUrl,
    sameOneUrl: sameOne.moduleUrl,
    twoUrl: two.moduleUrl,
    resolvedOne: import.meta.resolve("./worker-module-http-fragment-dep.js#one"),
    resolvedTwo: import.meta.resolve("./worker-module-http-fragment-dep.js#two"),
    oneFirst,
    oneSecond,
    sameOneCounter: sameOne.counter,
    twoFirst,
    twoCounter: two.counter,
  });
} catch (error) {
  postMessage({
    error: String(error && error.message ? error.message : error),
  });
}

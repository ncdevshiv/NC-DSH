onmessage = function (event) {
  switch (event.data) {
    case "syntax-error":
      try {
        importScripts(
          "data:text/javascript,globalThis.__syntaxFirst='ok'",
          "data:text/javascript,globalThis.__syntaxBroken = ;",
          "data:text/javascript,globalThis.__syntaxThird='unexpected'"
        );
        postMessage({ name: "unexpected-success" });
      } catch (error) {
        postMessage({
          first: globalThis.__syntaxFirst,
          hasThird: Object.prototype.hasOwnProperty.call(globalThis, "__syntaxThird"),
          name: error && error.name,
          syntax: error instanceof SyntaxError,
        });
      }
      close();
      return;
    case "runtime-throw":
      try {
        importScripts(
          "data:text/javascript,globalThis.__runtimeFirst=1",
          "data:text/javascript,throw 2",
          "data:text/javascript,globalThis.__runtimeThird=3"
        );
        postMessage({ name: "unexpected-success" });
      } catch (error) {
        postMessage({
          first: globalThis.__runtimeFirst,
          thrown: error,
          hasThird: Object.prototype.hasOwnProperty.call(globalThis, "__runtimeThird"),
        });
      }
      close();
      return;
    case "fetch-failure":
      try {
        importScripts(
          "./worker-importscripts-side-effect.js",
          "./worker-importscripts-missing.js",
          "./worker-importscripts-after.js"
        );
        postMessage({ name: "unexpected-success" });
      } catch (error) {
        postMessage({
          firstLoaded: globalThis.__fetchFirstLoaded,
          afterLoaded: globalThis.__fetchAfterLoaded === true,
          name: error && error.name,
          domException: error instanceof DOMException,
          message: String(error && error.message),
        });
      }
      close();
      return;
    default:
      postMessage({
        name: "unexpected-scenario",
        scenario: event.data,
      });
      close();
  }
};

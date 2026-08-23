onmessage = function (event) {
  switch (event.data.scenario) {
    case "same-origin-syntax":
      addEventListener(
        "error",
        function (errorEvent) {
          postMessage({
            name: errorEvent.error && errorEvent.error.name,
            messageIncludesSyntaxError: String(errorEvent.message).includes("SyntaxError"),
            filename: errorEvent.filename,
            lineno: errorEvent.lineno,
          });
          errorEvent.preventDefault();
          close();
        },
        { once: true },
      );
      importScripts("./worker-importscripts-reporting-syntax-error.js");
      return;
    case "same-origin-runtime":
      addEventListener(
        "error",
        function (errorEvent) {
          postMessage({
            name: errorEvent.error && errorEvent.error.name,
            errorMessage: errorEvent.error && errorEvent.error.message,
            filename: errorEvent.filename,
            lineno: errorEvent.lineno,
          });
          errorEvent.preventDefault();
          close();
        },
        { once: true },
      );
      importScripts("./worker-importscripts-reporting-runtime-error.js");
      return;
    case "cross-origin-helper":
      addEventListener(
        "error",
        function (errorEvent) {
          postMessage({
            name: errorEvent.error && errorEvent.error.name,
            domException: errorEvent.error instanceof DOMException,
            messageIsScriptError: errorEvent.message === "Script error.",
            filename: errorEvent.filename,
            lineno: errorEvent.lineno,
          });
          errorEvent.preventDefault();
          close();
        },
        { once: true },
      );
      function doImportScripts(url) {
        importScripts(url);
      }
      setTimeout(function () {
        doImportScripts(event.data.url);
      }, 0);
      return;
    default:
      postMessage({
        name: "unexpected-scenario",
        scenario: event.data.scenario,
      });
      close();
  }
};

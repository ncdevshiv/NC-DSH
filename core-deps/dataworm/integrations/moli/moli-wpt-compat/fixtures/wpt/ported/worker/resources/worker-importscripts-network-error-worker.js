onmessage = function (event) {
  const results = [];

  for (const url of event.data) {
    delete globalThis.__lmCrossOriginImportScriptsLoaded;
    try {
      importScripts(url);
      results.push({
        name: "unexpected-success",
        domException: false,
        loaded: globalThis.__lmCrossOriginImportScriptsLoaded === true,
      });
    } catch (error) {
      results.push({
        name: error && error.name,
        domException: error instanceof DOMException,
        loaded: globalThis.__lmCrossOriginImportScriptsLoaded === true,
      });
    }
  }

  postMessage(results);
  close();
};

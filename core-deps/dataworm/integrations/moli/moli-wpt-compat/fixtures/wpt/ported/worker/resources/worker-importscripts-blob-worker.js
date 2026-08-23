onmessage = function (event) {
  switch (event.data) {
    case "blob-load": {
      const blobUrl = URL.createObjectURL(new Blob([
        "globalThis.__blobImportedValue = 'blob-loaded';"
      ], { type: "text/javascript" }));
      const payload = {
        blobUrlPrefix: blobUrl.startsWith("blob:http://"),
        objectUrlType: typeof URL.createObjectURL,
      };
      importScripts(blobUrl);
      URL.revokeObjectURL(blobUrl);
      payload.imported = globalThis.__blobImportedValue;
      postMessage(payload);
      close();
      return;
    }
    case "revoked-blob": {
      const scriptUrl = URL.createObjectURL(new Blob([
        "globalThis.__revokedBlobRan = true;"
      ], { type: "text/javascript" }));
      URL.revokeObjectURL(scriptUrl);
      try {
        importScripts(scriptUrl);
        postMessage({
          name: "unexpected",
          ran: globalThis.__revokedBlobRan === true,
        });
      } catch (error) {
        postMessage({
          name: error && error.name,
          ran: globalThis.__revokedBlobRan === true,
        });
      }
      close();
      return;
    }
    case "prepared-revoke": {
      const runScriptUrl = URL.createObjectURL(new Blob([
        "globalThis.__preparedBlobRan = true;"
      ], { type: "text/javascript" }));
      const revokeScriptUrl = URL.createObjectURL(new Blob([
        "URL.revokeObjectURL(" + JSON.stringify(runScriptUrl) + ");"
      ], { type: "text/javascript" }));
      importScripts(revokeScriptUrl, runScriptUrl);
      postMessage({
        ran: globalThis.__preparedBlobRan === true,
      });
      close();
      return;
    }
    default:
      postMessage({
        name: "unexpected-scenario",
        scenario: event.data,
      });
      close();
  }
};

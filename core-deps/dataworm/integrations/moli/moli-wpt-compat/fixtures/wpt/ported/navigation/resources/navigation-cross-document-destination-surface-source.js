(function () {
  const config = globalThis.__lmNavigationCrossDocumentDestinationSurfaceConfig;
  if (!config) {
    return;
  }

  function key(suffix) {
    return config.storagePrefix + suffix;
  }

  for (const suffix of ["sourceLog", "resultOrder"]) {
    sessionStorage.removeItem(key(suffix));
  }

  const params = new URLSearchParams({
    mode: config.mode,
    sourcePath: config.sourcePath,
    storagePrefix: config.storagePrefix,
    testName: config.testName,
  });
  const destinationUrl =
    "resources/navigation-cross-document-destination-surface-dest.html?" + params.toString();

  const log = [];
  navigation.addEventListener("currententrychange", function (event) {
    log.push(
      "listener:" + String(event.navigationType) + ":" + (event.from ? event.from.url : "null"),
    );
    sessionStorage.setItem(key("sourceLog"), log.join("|"));
  });

  navigation.oncurrententrychange = function (event) {
    log.push(
      "prop:" + String(event.navigationType) + ":" + (event.from ? event.from.url : "null"),
    );
    sessionStorage.setItem(key("sourceLog"), log.join("|"));
  };

  let committedSettled = false;
  let finishedSettled = false;

  sessionStorage.setItem(key("sourceLog"), "before");
  const result =
    config.mode === "push"
      ? navigation.navigate(destinationUrl, { history: "push" })
      : config.mode === "replace"
        ? navigation.navigate(destinationUrl, { history: "replace" })
        : navigation.navigate(destinationUrl);
  sessionStorage.setItem(
    key("resultOrder"),
    [
      "surface:" +
        String(
          !!result &&
            typeof result.committed?.then === "function" &&
            typeof result.finished?.then === "function",
        ),
      "sync:" + String(committedSettled) + "," + String(finishedSettled),
    ].join("|"),
  );

  result.committed.then(function () {
    committedSettled = true;
    sessionStorage.setItem(
      key("resultOrder"),
      sessionStorage.getItem(key("resultOrder")) + "|committed",
    );
  });

  result.finished.then(function () {
    finishedSettled = true;
    sessionStorage.setItem(
      key("resultOrder"),
      sessionStorage.getItem(key("resultOrder")) + "|finished",
    );
  });

  sessionStorage.setItem(
    key("sourceLog"),
    sessionStorage.getItem(key("sourceLog")) + "|after-call",
  );
})();

(function () {
  const config =
    globalThis.__lmNavigationCrossDocumentCurrentEntryChangeQuietnessConfig;
  if (!config) {
    return;
  }

  function key(suffix) {
    return config.storagePrefix + suffix;
  }

  const params = new URLSearchParams({
    sourceUrl: location.href,
    storagePrefix: config.storagePrefix,
    testName: config.testName,
  });
  const destinationUrl =
    "resources/navigation-cross-document-currententrychange-quietness-dest.html?" +
    params.toString();

  sessionStorage.removeItem(key("navlog"));
  const log = [];

  navigation.addEventListener("currententrychange", function (event) {
    log.push(
      "listener:" +
        String(event.navigationType) +
        ":" +
        (event.from ? event.from.url : "null"),
    );
    sessionStorage.setItem(key("navlog"), log.join("|"));
  });

  navigation.oncurrententrychange = function (event) {
    log.push(
      "prop:" +
        String(event.navigationType) +
        ":" +
        (event.from ? event.from.url : "null"),
    );
    sessionStorage.setItem(key("navlog"), log.join("|"));
  };

  sessionStorage.setItem(key("navlog"), "before");
  navigation.navigate(destinationUrl);
  sessionStorage.setItem(
    key("navlog"),
    (sessionStorage.getItem(key("navlog")) ?? "") + "|after-call",
  );
})();

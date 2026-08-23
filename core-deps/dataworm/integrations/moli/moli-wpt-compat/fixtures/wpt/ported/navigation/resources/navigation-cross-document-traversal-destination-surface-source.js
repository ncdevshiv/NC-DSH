(function () {
  const config = globalThis.__lmNavigationCrossDocumentTraversalSurfaceConfig;
  if (!config) {
    return;
  }

  const storagePrefix = config.storagePrefix;
  const destinationUrl = new URL(
    "navigation-cross-document-traversal-destination-surface-dest.html?mode=" +
      encodeURIComponent(config.mode) +
      "&storagePrefix=" +
      encodeURIComponent(storagePrefix) +
      "&sourceUrl=" +
      encodeURIComponent(location.href),
    new URL("resources/", location.href),
  ).href;

  function key(suffix) {
    return storagePrefix + suffix;
  }

  function setData(name, value) {
    document.body.setAttribute("data-" + name, String(value));
  }

  let listenerCount = 0;
  let propertyCount = 0;

  navigation.addEventListener("currententrychange", function () {
    listenerCount += 1;
  });

  navigation.oncurrententrychange = function () {
    propertyCount += 1;
  };

  if (sessionStorage.getItem(key("stage")) !== "dest-ready") {
    for (const suffix of [
      "stage",
      "destBefore",
      "sourceSync",
      "sourceLog",
      "sourceCommittedUrl",
      "sourceFinishedUrl",
    ]) {
      sessionStorage.removeItem(key(suffix));
    }
    sessionStorage.setItem(key("stage"), "source-start");
    navigation.navigate(destinationUrl, { history: "push" });
    return;
  }

  sessionStorage.removeItem(key("stage"));

  const before = JSON.parse(sessionStorage.getItem(key("destBefore")) ?? "null");
  setData("transition-is-null", navigation.transition === null);
  setData("entry-url", navigation.activation?.entry?.url ?? "");
  setData("from-url", navigation.activation?.from?.url ?? "");
  setData("navigation-type", navigation.activation?.navigationType ?? "");
  setData("currententrychange-count", listenerCount);
  setData("property-currententrychange-count", propertyCount);
  setData("history-state", JSON.stringify(history.state));
  setData(
    "navigation-state",
    JSON.stringify(navigation.currentEntry?.getState?.() ?? null),
  );
  setData("b-entry-url", before?.activationEntry ?? "");
  setData("b-from-url", before?.activationFrom ?? "");
  setData("b-navigation-type", before?.activationType ?? "");
  setData("b-current-url", before?.current ?? "");
  setData("b-can-back", before?.canBack ?? "");
  setData("b-can-forward", before?.canForward ?? "");
  setData("source-sync", sessionStorage.getItem(key("sourceSync")) ?? "");
  setData("source-log", sessionStorage.getItem(key("sourceLog")) ?? "");
  setData(
    "source-committed-url",
    sessionStorage.getItem(key("sourceCommittedUrl")) ?? "",
  );
  setData(
    "source-finished-url",
    sessionStorage.getItem(key("sourceFinishedUrl")) ?? "",
  );

  test(function () {
    assert_equals(
      document.body.getAttribute("data-transition-is-null"),
      "true",
      "activation traversal should not expose a live transition in the destination document",
    );
    assert_equals(
      document.body.getAttribute("data-entry-url"),
      location.href,
      "activation entry URL should point at the re-activated source document",
    );
    assert_equals(
      document.body.getAttribute("data-from-url"),
      destinationUrl,
      "activation from URL should point at the traversed-away destination document",
    );
    assert_equals(
      document.body.getAttribute("data-navigation-type"),
      "traverse",
      "activation should report a traverse navigation on the source document",
    );
    assert_equals(
      document.body.getAttribute("data-currententrychange-count"),
      "0",
      "source document should not dispatch currententrychange during traversal bootstrap",
    );
    assert_equals(
      document.body.getAttribute("data-property-currententrychange-count"),
      "0",
      "source document oncurrententrychange should stay quiet during traversal bootstrap",
    );
    assert_equals(
      document.body.getAttribute("data-history-state"),
      "null",
      "cross-document traversal should not populate history.state by default",
    );
    assert_equals(
      document.body.getAttribute("data-navigation-state"),
      "null",
      "cross-document traversal should not populate navigation.currentEntry.getState() by default",
    );
    assert_equals(
      document.body.getAttribute("data-b-entry-url"),
      destinationUrl,
      "the destination snapshot should have observed itself as the activation entry",
    );
    assert_equals(
      document.body.getAttribute("data-b-from-url"),
      location.href,
      "the destination snapshot should have observed the source document as its activation.from URL",
    );
    assert_equals(
      document.body.getAttribute("data-b-navigation-type"),
      "push",
      "the destination snapshot should record the initial cross-document push activation",
    );
    assert_equals(
      document.body.getAttribute("data-b-current-url"),
      destinationUrl,
      "the destination snapshot should have seen its own currentEntry URL",
    );
    assert_equals(
      document.body.getAttribute("data-b-can-back"),
      "true",
      "the destination snapshot should still be able to traverse back to the source entry",
    );
    assert_equals(
      document.body.getAttribute("data-b-can-forward"),
      "false",
      "the destination snapshot should not expose a forward traversal target before traversing back",
    );
    assert_equals(
      document.body.getAttribute("data-source-sync"),
      "surface:true|sync:false,false",
      "traversal result promises should stay pending at the synchronous call boundary",
    );
    assert_equals(
      document.body.getAttribute("data-source-log"),
      "before|after-call",
      "traversal result promise callbacks should not run before the source document takes over again",
    );
    assert_equals(
      document.body.getAttribute("data-source-committed-url"),
      "null",
      "traversal committed promise should still be unresolved before the source document takes over",
    );
    assert_equals(
      document.body.getAttribute("data-source-finished-url"),
      "null",
      "traversal finished promise should still be unresolved before the source document takes over",
    );
  }, config.testName);
})();

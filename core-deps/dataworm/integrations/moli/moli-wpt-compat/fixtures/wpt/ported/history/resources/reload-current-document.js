(function () {
  const config = globalThis.__lmReloadCurrentDocumentConfig;
  if (!config) {
    return;
  }

  function key(suffix) {
    return config.storagePrefix + suffix;
  }

  setup({ explicit_done: true });

  function removeSnapshotKeys() {
    for (const suffix of [
      "count",
      "syncSnapshot",
      "returnShape",
      "committedShape",
      "finishedShape",
    ]) {
      sessionStorage.removeItem(key(suffix));
    }
  }

  function triggerReload() {
    if (config.mode === "history-go-zero") {
      history.go(0);
      return;
    }
    if (config.mode === "history-go-no-argument") {
      history.go();
      return;
    }
    if (config.mode === "history-go-nan") {
      history.go(NaN);
      return;
    }

    const result = navigation.reload({ state: { ignored: true } });
    sessionStorage.setItem(
      key("returnShape"),
      String(
        !!result &&
          typeof result === "object" &&
          typeof result.committed?.then === "function" &&
          typeof result.finished?.then === "function",
      ),
    );
    result.committed.then(function (entry) {
      sessionStorage.setItem(
        key("committedShape"),
        [
          Object.prototype.toString.call(entry),
          String(entry?.url ?? ""),
          String(entry === navigation.currentEntry),
        ].join("|"),
      );
    });
    result.finished.then(function (entry) {
      sessionStorage.setItem(
        key("finishedShape"),
        [
          Object.prototype.toString.call(entry),
          String(entry?.url ?? ""),
          String(entry === navigation.currentEntry),
        ].join("|"),
      );
    });
  }

  function run() {
    const count = Number(sessionStorage.getItem(key("count")) ?? "0") + 1;
    sessionStorage.setItem(key("count"), String(count));

    let listenerCount = 0;
    let propertyCount = 0;
    navigation.addEventListener("currententrychange", function () {
      listenerCount += 1;
    });
    navigation.oncurrententrychange = function () {
      propertyCount += 1;
    };

    if (count === 1) {
      removeSnapshotKeys();
      sessionStorage.setItem(key("count"), String(count));
      const beforeEntry = navigation.currentEntry;
      sessionStorage.setItem(
        key("syncSnapshot"),
        [
          String(beforeEntry === navigation.currentEntry),
          String(listenerCount),
          String(propertyCount),
        ].join("|"),
      );
      triggerReload();
      return;
    }

    test(function () {
      assert_equals(String(count), "2", "reload path should revisit the same document exactly once");
      assert_equals(location.href, navigation.currentEntry?.url, "reload should keep the current URL stable");
      assert_equals(
        sessionStorage.getItem(key("syncSnapshot")),
        "true|0|0",
        "reload should preserve the currentEntry object and stay quiet at the synchronous call boundary",
      );
      assert_equals(
        JSON.stringify(history.state),
        "null",
        "reload should not populate history.state by default",
      );
      assert_equals(
        JSON.stringify(navigation.currentEntry?.getState?.() ?? null),
        "null",
        "reload should not populate navigation state by default",
      );
      assert_equals(
        String(navigation.activation?.navigationType ?? ""),
        "reload",
        "reload should expose activation navigationType=reload",
      );
      assert_equals(
        navigation.activation?.entry?.url,
        location.href,
        "reload activation entry should point at the current document",
      );
      assert_equals(
        navigation.activation?.from?.url,
        location.href,
        "reload activation.from should point at the same current document URL",
      );
      assert_equals(
        String(listenerCount),
        "0",
        "reload should not dispatch currententrychange during bootstrap",
      );
      assert_equals(
        String(propertyCount),
        "0",
        "reload should keep oncurrententrychange quiet during bootstrap",
      );

      if (config.mode === "navigation-reload") {
        assert_equals(
          typeof navigation.reload,
          "function",
          "navigation.reload should be exposed as a function",
        );
        assert_equals(
          String(navigation.reload.length),
          "0",
          "navigation.reload.length should stay at 0",
        );
        assert_equals(
          sessionStorage.getItem(key("returnShape")),
          "true",
          "navigation.reload should return committed/finished promise surface",
        );
        assert_equals(
          sessionStorage.getItem(key("committedShape")) ?? "",
          "",
          "navigation.reload committed should remain unresolved before the reloaded document takes over",
        );
        assert_equals(
          sessionStorage.getItem(key("finishedShape")) ?? "",
          "",
          "navigation.reload finished should remain unresolved before the reloaded document takes over",
        );
      }
    }, config.testName);
    done();
  }

  if (document.readyState === "complete") {
    run();
    return;
  }

  addEventListener(
    "load",
    function () {
      run();
    },
    { once: true },
  );
})();

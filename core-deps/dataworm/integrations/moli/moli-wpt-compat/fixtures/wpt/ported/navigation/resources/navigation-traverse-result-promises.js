(function () {
  const config = globalThis.__lmNavigationTraverseResultPromisesConfig;
  if (!config) {
    return;
  }

  function waitForLoad() {
    if (document.readyState === "complete") {
      return Promise.resolve();
    }
    return new Promise(function (resolve) {
      addEventListener("load", resolve, { once: true });
    });
  }

  promise_test(async function () {
    await waitForLoad();

    history.pushState({ n: 1 }, "", "#one");
    history.pushState({ n: 2 }, "", "#two");

    const order = [];
    let committedSettled = false;
    let finishedSettled = false;
    let committedValue = null;
    let finishedValue = null;
    let resolveHashchangeCompletion = null;
    const hashchangeCompletion = new Promise(function (resolve) {
      resolveHashchangeCompletion = resolve;
    });
    const beforeEntry = navigation.currentEntry;

    navigation.addEventListener("currententrychange", function (event) {
      order.push(
        "cec:" +
          String(event.navigationType) +
          ":" +
          (event.from ? new URL(event.from.url).hash || "(none)" : "(null)"),
      );
      queueMicrotask(function () {
        order.push(
          "cec-micro:" +
            (new URL(location.href).hash || "(none)") +
            ":" +
            String(history.state?.n ?? "null"),
        );
      });
    });

    addEventListener("popstate", function (event) {
      order.push("popstate:" + String(event.state?.n ?? "null"));
      queueMicrotask(function () {
        order.push("popstate-micro:" + String(history.state?.n ?? "null"));
      });
    });

    addEventListener("hashchange", function () {
      order.push("hashchange:" + (new URL(location.href).hash || "(none)"));
      queueMicrotask(function () {
        order.push("hashchange-micro:" + (new URL(location.href).hash || "(none)"));
        resolveHashchangeCompletion();
      });
    });

    let result = null;
    if (config.mode === "back") {
      result = navigation.back();
    } else {
      const entries = navigation.entries();
      assert_true(
        entries.length >= 2,
        "traverseTo() needs a prior entry to restore",
      );
      const target = entries[entries.length - 2];
      assert_true(
        typeof target?.key === "string" && target.key.length > 0,
        "traverseTo() target should expose a non-empty key",
      );
      result = navigation.traverseTo(target.key);
    }

    result.committed.then(function (entry) {
      committedSettled = true;
      committedValue = entry;
      order.push(
        "committed:" +
          (new URL(entry.url).hash || "(none)") +
          ":" +
          String(entry === navigation.currentEntry) +
          ":" +
          String(entry === beforeEntry),
      );
    });

    result.finished.then(function (entry) {
      finishedSettled = true;
      finishedValue = entry;
      order.push(
        "finished:" +
          (new URL(entry.url).hash || "(none)") +
          ":" +
          String(entry === navigation.currentEntry) +
          ":" +
          String(entry === beforeEntry),
      );
    });

    assert_false(committedSettled, "committed should stay pending at the synchronous call boundary");
    assert_false(finishedSettled, "finished should stay pending at the synchronous call boundary");
    assert_equals(
      Object.keys(result).join(","),
      "committed,finished",
      "result should expose committed and finished enumerable keys",
    );
    assert_equals(
      Object.getOwnPropertyNames(result).join(","),
      "committed,finished",
      "result should only expose committed and finished own properties",
    );
    assert_true(
      navigation.currentEntry === beforeEntry,
      "currentEntry should still point at the pre-traversal entry at the synchronous boundary",
    );

    await hashchangeCompletion;

    assert_equals(location.hash, "#one", "traversal should restore the earlier fragment before hashchange completes");
    assert_true(committedSettled, "committed should settle after the traversal completes");
    assert_true(finishedSettled, "finished should settle after the traversal completes");
    assert_array_equals(
      order,
      [
        "cec:traverse:#two",
        "committed:#one:true:false",
        "cec-micro:#one:1",
        "finished:#one:true:false",
        "popstate:1",
        "popstate-micro:1",
        "hashchange:#one",
        "hashchange-micro:#one",
      ],
      "same-document traversal should preserve the current implementation's promise and event ordering",
    );
    assert_equals(
      Object.prototype.toString.call(committedValue),
      "[object NavigationHistoryEntry]",
      "committed should resolve to a NavigationHistoryEntry",
    );
    assert_equals(
      Object.prototype.toString.call(finishedValue),
      "[object NavigationHistoryEntry]",
      "finished should resolve to a NavigationHistoryEntry",
    );
    assert_true(
      committedValue === navigation.currentEntry,
      "committed should resolve with the restored currentEntry object",
    );
    assert_true(
      finishedValue === navigation.currentEntry,
      "finished should resolve with the restored currentEntry object",
    );
    assert_false(
      committedValue === beforeEntry,
      "committed should not resolve with the pre-traversal entry object",
    );
    assert_false(
      finishedValue === beforeEntry,
      "finished should not resolve with the pre-traversal entry object",
    );
  }, config.testName);
})();

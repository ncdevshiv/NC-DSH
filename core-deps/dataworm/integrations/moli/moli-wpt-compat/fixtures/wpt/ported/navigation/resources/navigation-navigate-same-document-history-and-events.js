(function () {
  const config = globalThis.__lmNavigationNavigateSameDocumentConfig;
  if (!config) {
    return;
  }

  const mode = config.mode;
  const targetHash = config.targetHash;
  const step = Number(config.step);
  const expectedLength = String(config.expectedLength);
  const expectedCanBack = String(config.expectedCanBack);
  const expectedCanForward = String(config.expectedCanForward);
  const testName = config.testName;

  promise_test(async function () {
    const order = [];
    let committed = false;
    let finished = false;

    addEventListener("popstate", function () {
      order.push("popstate:" + location.hash);
    });

    addEventListener("hashchange", function () {
      order.push("hashchange:" + location.hash);
    });

    navigation.addEventListener("currententrychange", function (event) {
      order.push(
        "cec:" +
          String(event.navigationType) +
          ":" +
          location.hash +
          ":" +
          (event.from ? new URL(event.from.url).hash || "(none)" : "(null)"),
      );
      queueMicrotask(function () {
        order.push(
          "cec-micro:" + location.hash + ":" + String(history.state?.step ?? "null"),
        );
      });
    });

    history.replaceState({ base: 1 }, "", "#base");

    const beforeLen = history.length;
    const beforeUrl = navigation.currentEntry.url;
    const result = navigation.navigate(location.pathname + targetHash, {
      history: mode,
      state: { step: step },
    });

    result.committed.then(function (entry) {
      committed = true;
      order.push(
        "committed:" +
          (new URL(entry.url).hash || "(none)") +
          ":" +
          String(entry === navigation.currentEntry),
      );
    });

    result.finished.then(function (entry) {
      finished = true;
      order.push(
        "finished:" +
          (new URL(entry.url).hash || "(none)") +
          ":" +
          String(entry === navigation.currentEntry),
      );
    });

    assert_equals(location.hash, targetHash, "navigate() should synchronously update location.hash");
    assert_equals(String(history.length), expectedLength, "history.length should reflect the requested history mode");
    assert_equals(
      new URL(navigation.currentEntry.url).hash || "(none)",
      targetHash,
      "currentEntry should synchronously point at the destination entry",
    );
    assert_equals(String(beforeLen), "1", "replaceState setup should keep history length at one entry");
    assert_equals(
      new URL(beforeUrl).hash || "(none)",
      "#base",
      "setup replaceState should leave the pre-navigation entry at #base",
    );
    assert_equals(JSON.stringify(history.state), "null", "history.state should stay null during navigation.navigate()");
    assert_equals(
      JSON.stringify(navigation.currentEntry.getState()),
      JSON.stringify({ step: step }),
      "currentEntry state should synchronously reflect the provided navigation state",
    );
    assert_true(
      !!result &&
        typeof result.committed?.then === "function" &&
        typeof result.finished?.then === "function",
      "navigate() should expose committed and finished promises",
    );
    assert_false(committed, "committed should stay pending at the synchronous call boundary");
    assert_false(finished, "finished should stay pending at the synchronous call boundary");
    assert_array_equals(
      order,
      [
        "cec:replace:#base:(none)",
        "cec:" + mode + ":" + targetHash + ":#base",
        "popstate:" + targetHash,
      ],
      "same-document navigate() should synchronously dispatch the expected currententrychange ordering",
    );

    await new Promise(function (resolve) {
      setTimeout(resolve, 0);
    });

    assert_equals(location.hash, targetHash, "destination hash should stay visible after the next macrotask");
    assert_equals(String(history.length), expectedLength, "history length should stay stable after the navigation settles");
    assert_equals(
      new URL(navigation.currentEntry.url).hash || "(none)",
      targetHash,
      "currentEntry should stay on the destination entry after the navigation settles",
    );
    assert_equals(JSON.stringify(history.state), "null", "history.state should remain null after the navigation settles");
    assert_equals(
      JSON.stringify(navigation.currentEntry.getState()),
      JSON.stringify({ step: step }),
      "currentEntry state should stay attached to the destination entry after the navigation settles",
    );
    assert_true(committed, "committed should settle after the synchronous call boundary");
    assert_true(finished, "finished should settle after the synchronous call boundary");
    assert_array_equals(
      order,
      [
        "cec:replace:#base:(none)",
        "cec:" + mode + ":" + targetHash + ":#base",
        "popstate:" + targetHash,
        "cec-micro:" + targetHash + ":null",
        "cec-micro:" + targetHash + ":null",
        "committed:" + targetHash + ":true",
        "finished:" + targetHash + ":true",
        "hashchange:" + targetHash,
      ],
      "same-document navigate() should preserve the expected microtask, promise, and hashchange ordering",
    );
    assert_equals(
      String(navigation.canGoBack),
      expectedCanBack,
      "canGoBack should reflect the settled session history state",
    );
    assert_equals(
      String(navigation.canGoForward),
      expectedCanForward,
      "canGoForward should reflect the settled session history state",
    );
  }, testName);
})();

(function () {
  const config = globalThis.__lmNavigationTraverseStateRestoreConfig;
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

  function nextMacrotask() {
    return new Promise(function (resolve) {
      setTimeout(resolve, 0);
    });
  }

  promise_test(async function () {
    await waitForLoad();

    navigation.updateCurrentEntry({ state: { nav: 0 } });
    let targetKey = "";
    if (config.mode === "traverse-to") {
      targetKey = navigation.currentEntry.key;
      assert_true(
        typeof targetKey === "string" && targetKey.length > 0,
        "traverseTo() should receive the current entry key for the restore target",
      );
    }

    history.pushState({ step: 1 }, "", "#one");

    const expectedFromUrl = new URL("#one", location.href).href;
    let eventNavigationType = "";
    let eventFromUrl = "";
    let eventHistoryState = "";
    let eventNavState = "";

    navigation.oncurrententrychange = function (event) {
      eventNavigationType = String(event.navigationType ?? "null");
      eventFromUrl = String(event.from?.url ?? "null");
      eventHistoryState = JSON.stringify(history.state);
      eventNavState = JSON.stringify(navigation.currentEntry.getState() ?? null);
    };

    if (config.mode === "back") {
      navigation.back();
    } else {
      navigation.traverseTo(targetKey);
    }

    assert_equals(location.hash, "#one", "traversal should synchronously expose the destination fragment");
    assert_equals(
      JSON.stringify(history.state),
      JSON.stringify({ step: 1 }),
      "history.state should synchronously expose the history entry payload",
    );
    assert_equals(
      JSON.stringify(navigation.currentEntry.getState() ?? null),
      "null",
      "navigation state should not restore until the traversal completes",
    );

    await nextMacrotask();

    assert_equals(location.hash, "", "traversal should restore the base entry after a macrotask");
    assert_equals(JSON.stringify(history.state), "null", "history.state should restore to the base entry payload");
    assert_equals(
      JSON.stringify(navigation.currentEntry.getState() ?? null),
      JSON.stringify({ nav: 0 }),
      "navigation state should restore from the base entry independently of history.state",
    );
    assert_equals(
      eventNavigationType,
      "traverse",
      "currententrychange should report a traverse navigation",
    );
    assert_equals(
      eventFromUrl,
      expectedFromUrl,
      "currententrychange.from should point at the traversed-away fragment entry",
    );
    assert_equals(
      eventHistoryState,
      "null",
      "history.state should already point at the restored entry inside currententrychange",
    );
    assert_equals(
      eventNavState,
      JSON.stringify({ nav: 0 }),
      "navigation.currentEntry.getState() should already point at the restored entry inside currententrychange",
    );
  }, config.testName);
})();

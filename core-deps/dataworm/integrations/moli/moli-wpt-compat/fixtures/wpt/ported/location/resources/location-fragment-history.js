(function () {
  const config = globalThis.__lmLocationFragmentHistoryConfig;
  if (!config) {
    return;
  }

  const initialHref = location.href;
  const expectedHref = initialHref + "#frag";
  const initialLength = history.length;
  const order = [];
  let popstateState = "missing";
  let oldUrl = "missing";
  let newUrl = "missing";

  addEventListener("popstate", function (event) {
    order.push("popstate");
    popstateState = JSON.stringify(event.state);
    queueMicrotask(function () {
      order.push("popstate-microtask");
    });
  });

  addEventListener("hashchange", function (event) {
    order.push("hashchange");
    oldUrl = event.oldURL;
    newUrl = event.newURL;
    queueMicrotask(function () {
      order.push("hashchange-microtask");
    });
  });

  promise_test(async function () {
    if (document.readyState !== "complete") {
      await new Promise(function (resolve) {
        addEventListener("load", function () {
          setTimeout(resolve, 0);
        }, { once: true });
      });
    }

    if (config.mode === "assign") {
      location.assign("#frag");
    } else if (config.mode === "replace") {
      location.replace("#frag");
    } else {
      location.href = "#frag";
    }

    order.push("after-call");

    const syncHref = location.href;
    const syncLength = history.length;
    const syncHash = location.hash;
    const syncHistoryState = JSON.stringify(history.state);

    await new Promise(function (resolve) {
      setTimeout(resolve, 0);
    });

    assert_array_equals(
      order,
      ["popstate", "after-call", "popstate-microtask", "hashchange", "hashchange-microtask"],
      "fragment navigation should preserve the expected event and microtask ordering",
    );
    assert_equals(syncHref, expectedHref, "fragment navigation should synchronously update location.href");
    assert_equals(location.href, expectedHref, "fragment navigation should commit the fragment URL");
    assert_equals(syncHash, "#frag", "fragment navigation should synchronously update location.hash");
    assert_equals(location.hash, "#frag", "fragment navigation should leave the fragment visible");
    assert_equals(oldUrl, initialHref, "hashchange.oldURL should point at the pre-navigation URL");
    assert_equals(newUrl, expectedHref, "hashchange.newURL should point at the fragment URL");
    assert_equals(popstateState, "null", "same-document fragment navigation should surface null popstate state");
    assert_equals(syncHistoryState, "null", "fragment navigation should not populate history.state");
    assert_equals(history.state, null, "fragment navigation should leave history.state null");
    assert_true(document.location === window.location, "document.location should alias window.location");

    if (config.mode === "replace") {
      assert_equals(
        syncLength,
        initialLength,
        "location.replace('#frag') should synchronously reuse the current session history entry",
      );
      assert_equals(
        history.length,
        initialLength,
        "location.replace('#frag') should keep the final session history length unchanged",
      );
    } else {
      assert_equals(
        syncLength,
        initialLength + 1,
        "post-load fragment navigation should synchronously extend session history for assign/href",
      );
      assert_equals(
        history.length,
        initialLength + 1,
        "post-load fragment navigation should leave the new fragment entry visible in session history",
      );
    }
  }, config.testName);
})();

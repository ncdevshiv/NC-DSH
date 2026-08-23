(function () {
  const config = globalThis.__lmHistoryFragmentTraversalConfig;
  if (!config) {
    return;
  }

  function wait(ms) {
    return new Promise(function (resolve) {
      setTimeout(resolve, ms);
    });
  }

  function wait_for_hash(expectedHash, trigger, description) {
    return Promise.race([
      new Promise(function (resolve) {
        function onHashchange() {
          if (location.hash !== expectedHash) {
            return;
          }
          removeEventListener("hashchange", onHashchange);
          resolve();
        }

        addEventListener("hashchange", onHashchange);
        trigger();
      }),
      wait(1000).then(function () {
        throw new Error("timed out waiting for " + description);
      }),
    ]);
  }

  promise_test(async function () {
    history.pushState({ step: 1 }, "", "#one");
    history.pushState({ step: 2 }, "", "#two");

    if (config.mode === "forward") {
      await wait_for_hash("#one", function () {
        history.back();
      }, "initial fragment back traversal");
    }

    const order = [];
    let oldUrl = "null";
    let newUrl = "null";
    let fromUrl = "null";
    let navigationType = "null";

    navigation.addEventListener(
      "currententrychange",
      function (event) {
        order.push(
          "currententrychange:" +
            String(event.navigationType) +
            ":" +
            (event.from ? new URL(event.from.url).hash || "(none)" : "(null)"),
        );
        queueMicrotask(function () {
          order.push(
            "currententrychange-microtask:" +
              location.hash +
              ":" +
              String(history.state?.step ?? "null"),
          );
        });
        fromUrl = event.from ? event.from.url : "null";
        navigationType = String(event.navigationType);
      },
      { once: true },
    );

    addEventListener(
      "popstate",
      function (event) {
        order.push("popstate:" + String(event.state?.step ?? "null"));
        queueMicrotask(function () {
          order.push("popstate-microtask:" + String(history.state?.step ?? "null"));
        });
      },
      { once: true },
    );

    addEventListener(
      "hashchange",
      function (event) {
        oldUrl = event.oldURL;
        newUrl = event.newURL;
        order.push("hashchange:" + location.hash);
        queueMicrotask(function () {
          order.push("hashchange-microtask:" + location.hash);
        });
      },
      { once: true },
    );

    const expectedHash = config.mode === "back" ? "#one" : "#two";
    const traversalCompleted = wait_for_hash(
      expectedHash,
      function () {
        if (config.mode === "back") {
          history.back();
        } else {
          history.forward();
        }
      },
      "final fragment traversal",
    );

    const syncHash = location.hash;
    const syncState = String(history.state?.step ?? "null");

    await traversalCompleted;

    assert_equals(
      syncHash,
      config.mode === "back" ? "#two" : "#one",
      "traversal should remain asynchronous during the initiating call",
    );
    assert_equals(
      syncState,
      config.mode === "back" ? "2" : "1",
      "history.state should still reflect the pre-traversal entry synchronously",
    );
    assert_equals(
      location.hash,
      config.mode === "back" ? "#one" : "#two",
      "traversal should eventually commit the target fragment",
    );
    assert_equals(
      String(history.state?.step ?? "null"),
      config.mode === "back" ? "1" : "2",
      "history.state should eventually follow the traversed entry",
    );
    assert_array_equals(
      order,
      config.mode === "back"
        ? [
            "currententrychange:traverse:#two",
            "currententrychange-microtask:#one:1",
            "popstate:1",
            "popstate-microtask:1",
            "hashchange:#one",
            "hashchange-microtask:#one",
          ]
        : [
            "currententrychange:traverse:#one",
            "currententrychange-microtask:#two:2",
            "popstate:2",
            "popstate-microtask:2",
            "hashchange:#two",
            "hashchange-microtask:#two",
          ],
      "fragment traversal should preserve currententrychange, popstate, and hashchange ordering",
    );
    assert_equals(navigationType, "traverse", "fragment traversal should surface navigationType=traverse");
    assert_equals(
      fromUrl,
      config.mode === "back" ? initialHrefWithHash("#two") : initialHrefWithHash("#one"),
      "currententrychange.from should point at the pre-traversal entry",
    );
    assert_equals(
      oldUrl,
      config.mode === "back" ? initialHrefWithHash("#two") : initialHrefWithHash("#one"),
      "hashchange.oldURL should point at the pre-traversal URL",
    );
    assert_equals(
      newUrl,
      config.mode === "back" ? initialHrefWithHash("#one") : initialHrefWithHash("#two"),
      "hashchange.newURL should point at the traversed URL",
    );
  }, config.testName);

  function initialHrefWithHash(hash) {
    return location.href.replace(/#.*$/, "") + hash;
  }
})();

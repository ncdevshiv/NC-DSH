(function () {
  const config = globalThis.__lmNavigationTraverseCurrentEntryChangeConfig;
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
    history.pushState({ n: 1 }, "", "#one");
    history.pushState({ n: 2 }, "", "#two");

    if (config.mode === "forward") {
      await wait_for_hash("#one", function () {
        history.back();
      }, "initial fragment back traversal before navigation.forward()");
      await wait(0);
    }

    const order = [];
    let fired = false;
    let fromUrl = "null";
    let navigationType = "null";
    const eventPromise = new Promise(function (resolve) {
      navigation.oncurrententrychange = function (event) {
        fired = true;
        fromUrl = event.from ? event.from.url : "null";
        navigationType = String(event.navigationType);
        order.push("currententrychange");
        queueMicrotask(function () {
          order.push("currententrychange-microtask:" + location.hash);
        });
        resolve(event);
      };
    });

    if (config.mode === "traverseTo") {
      const target = navigation
        .entries()
        .find(function (entry) {
          return new URL(entry.url).hash === "#one";
        });
      assert_true(!!target, "traverseTo() test should find the prior fragment entry");
      navigation.traverseTo(target.key);
    } else {
      navigation.forward();
    }

    const syncHash = location.hash;
    const syncState = JSON.stringify(history.state);

    await eventPromise;
    await wait(0);

    assert_true(fired, "oncurrententrychange should fire for same-document traversal");
    assert_equals(
      syncHash,
      config.mode === "traverseTo" ? "#two" : "#one",
      "traversal should remain asynchronous during the initiating call",
    );
    assert_equals(
      syncState,
      config.mode === "traverseTo" ? "{\"n\":2}" : "{\"n\":1}",
      "history.state should still reflect the pre-traversal entry synchronously",
    );
    assert_equals(
      location.hash,
      config.mode === "traverseTo" ? "#one" : "#two",
      "traversal should eventually commit the target fragment",
    );
    assert_equals(
      JSON.stringify(history.state),
      config.mode === "traverseTo" ? "{\"n\":1}" : "{\"n\":2}",
      "history.state should eventually follow the traversed entry",
    );
    assert_array_equals(
      order,
      config.mode === "traverseTo"
        ? ["currententrychange", "currententrychange-microtask:#one"]
        : ["currententrychange", "currententrychange-microtask:#two"],
      "oncurrententrychange should run once and observe the traversed fragment by microtask time",
    );
    assert_equals(navigationType, "traverse", "same-document traversal should surface navigationType=traverse");
    assert_equals(
      fromUrl,
      config.mode === "traverseTo" ? initialHrefWithHash("#two") : initialHrefWithHash("#one"),
      "event.from should point at the pre-traversal entry",
    );
  }, config.testName);

  function initialHrefWithHash(hash) {
    return location.href.replace(/#.*$/, "") + hash;
  }
})();

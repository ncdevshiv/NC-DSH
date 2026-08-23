(function () {
  function snapshot() {
    const activation = navigation.activation;
    return [
      "href=" + location.href,
      "activationEntry=" + String(activation?.entry?.url ?? ""),
      "activationFrom=" + String(activation?.from?.url ?? ""),
      "activationType=" + String(activation?.navigationType ?? ""),
      "current=" + String(navigation.currentEntry?.url ?? ""),
      "transitionIsNull=" + String(navigation.transition === null),
    ].join("|");
  }

  function nextTask(delay) {
    return new Promise(function (resolve) {
      setTimeout(resolve, delay ?? 0);
    });
  }

  function waitForHash(expectedHash) {
    return new Promise(function (resolve) {
      function onHashChange() {
        if (location.hash !== expectedHash) {
          return;
        }
        removeEventListener("hashchange", onHashChange);
        resolve();
      }

      addEventListener("hashchange", onHashChange);
    });
  }

  promise_test(async function () {
    const base = location.href;
    const expectedBase =
      "href=" +
      base +
      "|activationEntry=" +
      base +
      "|activationFrom=|activationType=push|current=" +
      base +
      "|transitionIsNull=true";
    const expectedHash =
      "href=" +
      base +
      "#one|activationEntry=" +
      base +
      "|activationFrom=|activationType=push|current=" +
      base +
      "#one|transitionIsNull=true";

    assert_equals(
      snapshot(),
      expectedBase,
      "initial activation should describe the initial document",
    );

    navigation.navigate("#one", { history: "push", state: { step: 1 } });
    assert_equals(
      snapshot(),
      expectedHash,
      "same-document push should update currentEntry without mutating activation",
    );

    await nextTask(0);
    assert_equals(
      snapshot(),
      expectedHash,
      "activation should stay initial after same-document push settles",
    );

    const backDone = waitForHash("");
    history.back();
    await backDone;
    await nextTask(0);
    assert_equals(
      snapshot(),
      expectedBase,
      "activation should stay initial after traversing back to the initial entry",
    );

    const forwardDone = waitForHash("#one");
    history.forward();
    await forwardDone;
    await nextTask(0);
    assert_equals(
      snapshot(),
      expectedHash,
      "activation should stay initial after traversing forward to the pushed entry",
    );

    const target = navigation.entries().find(function (entry) {
      return new URL(entry.url).hash === "";
    });
    assert_true(Boolean(target), "navigation.entries() should expose the initial entry for traverseTo()");

    const traverseDone = waitForHash("");
    navigation.traverseTo(target.key);
    await traverseDone;
    await nextTask(50);
    assert_equals(
      snapshot(),
      expectedBase,
      "activation should stay initial after traverseTo() returns to the initial entry",
    );
  }, "same-document push/back/forward/traverse should not mutate navigation.activation");
})();

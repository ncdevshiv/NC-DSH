(function () {
  const config = globalThis.__lmLocationSearchConfig;
  if (!config) {
    return;
  }

  const stage = new URL(location.href).searchParams.get("stage");

  function key(suffix) {
    return config.storagePrefix + suffix;
  }

  if (stage !== "final") {
    for (const suffix of ["initialHref", "initialLength", "fromHref", "sourceLoaded"]) {
      sessionStorage.removeItem(key(suffix));
    }

    sessionStorage.setItem(key("initialHref"), location.href);
    sessionStorage.setItem(key("initialLength"), String(history.length));

    if (config.mode === "post-load") {
      addEventListener(
        "load",
        function () {
          setTimeout(function () {
            sessionStorage.setItem(key("sourceLoaded"), "true");
            sessionStorage.setItem(key("fromHref"), location.href);
            location.search = config.targetSearch;
          }, 0);
        },
        { once: true },
      );
      return;
    }

    sessionStorage.setItem(key("fromHref"), location.href);
    location.search = config.targetSearch;
    return;
  }

  test(function () {
    const initialHref = sessionStorage.getItem(key("initialHref"));
    const initialLength = Number(sessionStorage.getItem(key("initialLength")));
    const fromHref = sessionStorage.getItem(key("fromHref"));
    const navigationState = JSON.stringify(navigation.currentEntry?.getState?.() ?? null);
    const currentUrl = new URL(location.href);
    const sourceUrl = new URL(fromHref ?? "about:blank");

    assert_true(initialHref !== null, "source stage should store the initial href");
    assert_true(fromHref !== null, "source stage should store the pre-navigation href");
    assert_true(initialHref !== location.href, "location.search should commit a new destination URL");
    assert_equals(
      sourceUrl.pathname,
      location.pathname,
      "location.search should preserve the current pathname",
    );
    assert_equals(
      sourceUrl.search,
      "",
      "source stage should start without a committed query string",
    );
    assert_equals(
      currentUrl.searchParams.get("marker"),
      config.marker,
      "destination URL should include the configured marker",
    );
    assert_equals(
      currentUrl.searchParams.get("from"),
      config.from,
      "destination URL should expose the expected query payload",
    );
    assert_true(document.location === window.location, "document.location should alias window.location");
    assert_equals(typeof location.assign, "function", "location.assign should stay callable");
    assert_equals(typeof location.replace, "function", "location.replace should stay callable");
    assert_equals(typeof location.reload, "function", "location.reload should stay callable");

    if (config.mode === "post-load") {
      assert_equals(
        sessionStorage.getItem(key("sourceLoaded")),
        "true",
        "post-load branch should wait for load before mutating location.search",
      );
      assert_equals(
        history.length,
        initialLength + 1,
        "post-load location.search should grow session history by one entry",
      );
      assert_equals(
        navigation.activation?.entry?.url ?? "",
        location.href,
        "activation entry URL should point at the committed destination",
      );
      assert_equals(
        navigation.activation?.from?.url ?? "",
        fromHref,
        "activation from URL should point at the source document",
      );
      assert_equals(
        navigation.activation?.navigationType ?? "",
        "push",
        "post-load location.search should report push activation",
      );
      assert_equals(history.state, null, "location.search should not populate history.state by default");
      assert_equals(
        navigationState,
        "null",
        "location.search should not populate navigation.currentEntry state by default",
      );
      return;
    }

    assert_true(
      history.length >= initialLength,
      "load-time location.search should leave session history in a valid state",
    );
  }, config.testName);
})();

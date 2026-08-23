(function () {
  const config = globalThis.__lmLocationPortConfig;
  if (!config) {
    return;
  }

  const url = new URL(location.href);
  const stage = url.searchParams.get("stage");

  if (stage !== "final") {
    const expectedPort = String(config.alternatePort ?? "");

    url.searchParams.set("stage", "final");
    url.searchParams.set("mode", "port");
    url.searchParams.set("initialLength", String(history.length));
    url.searchParams.set("sourcePort", location.port);
    url.searchParams.set("sourceHost", location.host);
    url.searchParams.set("expectedPort", expectedPort);
    history.replaceState(null, "", url.href);

    location.port = expectedPort;
    return;
  }

  test(function () {
    const initialLength = Number(url.searchParams.get("initialLength"));
    const sourcePort = url.searchParams.get("sourcePort");
    const sourceHost = url.searchParams.get("sourceHost");
    const expectedPort = url.searchParams.get("expectedPort");

    assert_equals(
      url.searchParams.get("mode"),
      "port",
      "destination URL should preserve the mode marker",
    );
    assert_true(sourcePort !== null, "source stage should encode the original port");
    assert_true(sourceHost !== null, "source stage should encode the original host");
    assert_true(expectedPort !== null, "source stage should encode the expected port");
    assert_true(
      sourcePort !== expectedPort,
      "source stage should encode a distinct alternate port target",
    );
    assert_true(location.port !== sourcePort, "location.port should commit a new destination port");
    assert_true(location.host !== sourceHost, "location.port should commit a new destination host");
    assert_equals(location.port, expectedPort, "destination should land on the expected port");
    assert_equals(
      location.host,
      location.hostname + ":" + expectedPort,
      "destination host should reflect the committed alternate port",
    );
    assert_equals(
      new URL(location.href).port,
      expectedPort,
      "final URL port should match the expected component target",
    );
    assert_true(
      location.pathname.indexOf("/ported/location/") !== -1,
      "navigation should stay on the shared compat fixture path",
    );
    assert_equals(
      url.searchParams.get("stage"),
      "final",
      "final URL should preserve the stage marker across port navigation",
    );
    assert_true(
      history.length >= initialLength,
      "port component navigation should leave session history in a valid state",
    );
    assert_true(document.location === window.location, "document.location should alias window.location");
    assert_equals(typeof location.assign, "function", "location.assign should stay callable");
    assert_equals(typeof location.replace, "function", "location.replace should stay callable");
    assert_equals(typeof location.reload, "function", "location.reload should stay callable");
  }, config.testName);
})();

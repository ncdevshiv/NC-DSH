(function () {
  const config = globalThis.__lmLocationHostConfig;
  if (!config) {
    return;
  }

  function alternateHostname(hostname) {
    return hostname === "localhost" ? "127.0.0.1" : "localhost";
  }

  const url = new URL(location.href);
  const stage = url.searchParams.get("stage");

  if (stage !== "final") {
    const expectedHostname = alternateHostname(location.hostname);
    const expectedHost = `${expectedHostname}:${location.port}`;

    url.searchParams.set("stage", "final");
    url.searchParams.set("mode", config.mode);
    url.searchParams.set("initialLength", String(history.length));
    url.searchParams.set("sourceHost", location.host);
    url.searchParams.set("sourceHostname", location.hostname);
    url.searchParams.set("expectedHost", expectedHost);
    url.searchParams.set("expectedHostname", expectedHostname);
    history.replaceState(null, "", url.href);

    if (config.mode === "host") {
      location.host = expectedHost;
    } else {
      location.hostname = expectedHostname;
    }
    return;
  }

  test(function () {
    const initialLength = Number(url.searchParams.get("initialLength"));
    const sourceHost = url.searchParams.get("sourceHost");
    const sourceHostname = url.searchParams.get("sourceHostname");
    const expectedHost = url.searchParams.get("expectedHost");
    const expectedHostname = url.searchParams.get("expectedHostname");

    assert_equals(url.searchParams.get("mode"), config.mode, "destination URL should preserve the mode marker");
    assert_true(sourceHost !== null, "source stage should encode the original host");
    assert_true(sourceHostname !== null, "source stage should encode the original hostname");
    assert_true(expectedHost !== null, "source stage should encode the expected host");
    assert_true(expectedHostname !== null, "source stage should encode the expected hostname");
    assert_true(location.host !== sourceHost, "location component navigation should commit a new host");
    assert_true(
      location.hostname !== sourceHostname,
      "location component navigation should commit a new hostname",
    );
    assert_equals(location.host, expectedHost, "destination should land on the expected host");
    assert_equals(
      location.hostname,
      expectedHostname,
      "destination should land on the expected hostname",
    );
    assert_equals(
      new URL(location.href).host,
      expectedHost,
      "final URL host should match the expected component target",
    );
    assert_true(
      location.pathname.indexOf("/ported/location/") !== -1,
      "navigation should stay on the shared compat fixture path",
    );
    assert_equals(
      url.searchParams.get("stage"),
      "final",
      "final URL should preserve the stage marker across host navigation",
    );
    assert_true(
      history.length >= initialLength,
      "host component navigation should leave session history in a valid state",
    );
    assert_true(document.location === window.location, "document.location should alias window.location");
    assert_equals(typeof location.assign, "function", "location.assign should stay callable");
    assert_equals(typeof location.replace, "function", "location.replace should stay callable");
    assert_equals(typeof location.reload, "function", "location.reload should stay callable");
  }, config.testName);
})();

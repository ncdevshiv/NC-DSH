(function () {
  const config = globalThis.__lmLocationPostLoadConfig;
  if (!config) {
    return;
  }

  function key(suffix) {
    return config.storagePrefix + suffix;
  }

  for (const suffix of ["initialHref", "initialLength", "fromHref", "sourceLoaded"]) {
    sessionStorage.removeItem(key(suffix));
  }

  sessionStorage.setItem(key("initialHref"), location.href);
  sessionStorage.setItem(key("initialLength"), String(history.length));

  const destination = new URL(
    "location-post-load-destination.html",
    new URL("resources/", location.href),
  );
  const params = new URLSearchParams({
    mode: config.mode,
    storagePrefix: config.storagePrefix,
    testName: config.testName,
  });

  if (config.mode === "pathname") {
    history.replaceState(null, "", location.pathname + "?" + params.toString());
    destination.search = location.search;
  } else {
    destination.search = params.toString();
  }

  addEventListener(
    "load",
    function () {
      setTimeout(function () {
        sessionStorage.setItem(key("sourceLoaded"), "true");
        sessionStorage.setItem(key("fromHref"), location.href);

        if (config.mode === "assign") {
          location.assign(destination.href);
        } else if (config.mode === "replace") {
          location.replace(destination.href);
        } else if (config.mode === "href") {
          location.href = destination.href;
        } else if (config.mode === "pathname") {
          location.pathname = destination.pathname;
        }
      }, 0);
    },
    { once: true },
  );
})();

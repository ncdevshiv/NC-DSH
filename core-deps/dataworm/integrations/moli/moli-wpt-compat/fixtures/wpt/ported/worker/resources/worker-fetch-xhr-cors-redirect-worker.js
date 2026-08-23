function alternate_url(port, path) {
  return location.protocol + "//" + location.hostname + ":" + port + path;
}

function post_error(kind, error) {
  postMessage({
    kind,
    error: String(error),
    name: error && error.name,
  });
}

function xhr_result(url, withCredentials) {
  return new Promise(function (resolve) {
    const events = [];
    const xhr = new XMLHttpRequest();
    xhr.onload = function () {
      events.push("load");
    };
    xhr.onerror = function () {
      events.push("error");
    };
    xhr.onloadend = function () {
      events.push("loadend");
      resolve({
        events,
        status: xhr.status,
        responseText: xhr.responseText,
        responseURL: xhr.responseURL,
      });
    };
    xhr.open("GET", url);
    xhr.withCredentials = !!withCredentials;
    xhr.send();
  });
}

async function fetch_same_to_cross() {
  const response = await fetch("/wpt/runtime/cors/redirect/to-credentials-allow", {
    credentials: "include",
  });
  return {
    type: response.type,
    redirected: response.redirected,
    url: response.url,
    text: await response.text(),
  };
}

async function xhr_same_to_cross() {
  return xhr_result("/wpt/runtime/cors/redirect/to-credentials-allow", true);
}

async function fetch_cross_to_same(alternatePort) {
  const response = await fetch(
    alternate_url(alternatePort, "/wpt/runtime/cors/redirect/to-same-origin-echo"),
  );
  return {
    type: response.type,
    redirected: response.redirected,
    url: response.url,
    payload: JSON.parse(await response.text()),
  };
}

async function xhr_cross_to_same(alternatePort) {
  const result = await xhr_result(
    alternate_url(alternatePort, "/wpt/runtime/cors/redirect/to-same-origin-echo"),
  );
  result.payload = JSON.parse(result.responseText);
  return result;
}

async function fetch_cross_site_chain_to_same() {
  const response = await fetch(
    "/wpt/runtime/cors/redirect/to-cross-site-to-same-origin-echo",
  );
  return {
    type: response.type,
    redirected: response.redirected,
    url: response.url,
    payload: JSON.parse(await response.text()),
  };
}

async function xhr_cross_site_chain_to_same() {
  const result = await xhr_result(
    "/wpt/runtime/cors/redirect/to-cross-site-to-same-origin-echo",
  );
  result.payload = JSON.parse(result.responseText);
  return result;
}

self.onmessage = async function (event) {
  const kind = event.data && event.data.kind;
  const alternatePort = event.data && event.data.alternatePort;
  try {
    let result;
    if (kind === "fetch-same-to-cross") {
      result = await fetch_same_to_cross();
    } else if (kind === "xhr-same-to-cross") {
      result = await xhr_same_to_cross();
    } else if (kind === "fetch-cross-to-same") {
      result = await fetch_cross_to_same(alternatePort);
    } else if (kind === "xhr-cross-to-same") {
      result = await xhr_cross_to_same(alternatePort);
    } else if (kind === "fetch-cross-site-chain-to-same") {
      result = await fetch_cross_site_chain_to_same();
    } else if (kind === "xhr-cross-site-chain-to-same") {
      result = await xhr_cross_site_chain_to_same();
    } else {
      throw new Error("unknown worker CORS redirect test kind");
    }
    postMessage(Object.assign({ kind }, result));
  } catch (error) {
    post_error(kind, error);
  }
};

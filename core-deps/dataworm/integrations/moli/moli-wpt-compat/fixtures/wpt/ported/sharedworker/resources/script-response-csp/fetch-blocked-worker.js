onconnect = event => {
  const port = event.ports[0];
  fetch("/wpt/ported/sharedworker/resources/script-response-csp/connect-target.txt")
    .then(() => port.postMessage("unexpected-fetch"))
    .catch(() => port.postMessage("blocked"));
};

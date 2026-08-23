importScripts("script.js");

onconnect = function (event) {
  event.ports[0].postMessage({
    href: location.href,
    result: result,
  });
};

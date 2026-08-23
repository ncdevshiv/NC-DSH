onconnect = function (event) {
  event.ports[0].postMessage({
    href: location.href,
    hash: location.hash,
  });
};

onconnect = function (event) {
  var xhr = new XMLHttpRequest();
  xhr.open("GET", "test.txt", false);
  xhr.send();
  event.ports[0].postMessage({
    href: location.href,
    response: xhr.responseText,
  });
};

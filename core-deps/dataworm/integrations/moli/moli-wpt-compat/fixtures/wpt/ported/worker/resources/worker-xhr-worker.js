self.onmessage = function (event) {
  const mode = event.data && event.data.mode;

  if (mode === "basic") {
    const xhr = new XMLHttpRequest();
    const events = [];
    xhr.onreadystatechange = function (currentEvent) {
      events.push("prop-rs:" + currentEvent.type + ":" + xhr.readyState);
    };
    xhr.onload = function () {
      events.push("prop-load:" + xhr.status);
    };
    xhr.onloadend = function () {
      events.push("prop-loadend:" + xhr.readyState);
    };
    xhr.addEventListener("readystatechange", function (currentEvent) {
      events.push("rs:" + currentEvent.type + ":" + xhr.readyState);
    });
    xhr.addEventListener("load", function () {
      events.push("load:" + xhr.status + ":" + xhr.responseText.trim());
    });
    xhr.addEventListener("loadend", function () {
      events.push("loadend:" + xhr.readyState);
      postMessage({
        mode: mode,
        ctor: typeof XMLHttpRequest,
        eventTarget: xhr instanceof XMLHttpRequestEventTarget,
        uploadTag: Object.prototype.toString.call(xhr.upload),
        status: xhr.status,
        url: xhr.responseURL,
        text: xhr.responseText.trim(),
        readyState: xhr.readyState,
        events: events,
      });
      close();
    });
    xhr.open("GET", "./worker-xhr-target.js");
    xhr.send();
    return;
  }

  if (mode === "abort") {
    const xhr = new XMLHttpRequest();
    const events = [];
    xhr.addEventListener("abort", function () {
      events.push("abort");
    });
    xhr.addEventListener("error", function () {
      events.push("error:" + xhr.readyState + ":" + xhr.status);
    });
    xhr.addEventListener("load", function () {
      events.push("load");
    });
    xhr.addEventListener("loadend", function () {
      events.push("loadend:" + xhr.readyState + ":" + xhr.status);
      setTimeout(function () {
        postMessage({
          mode: mode,
          readyState: xhr.readyState,
          status: xhr.status,
          responseText: xhr.responseText,
          responseURL: xhr.responseURL,
          events: events,
        });
        close();
      }, 150);
    });
    xhr.open("GET", "/wpt/runtime/xhr/slow");
    xhr.send();
    setTimeout(function () {
      xhr.abort();
    }, 0);
    return;
  }

  postMessage({ mode: mode, error: "unknown mode" });
  close();
};

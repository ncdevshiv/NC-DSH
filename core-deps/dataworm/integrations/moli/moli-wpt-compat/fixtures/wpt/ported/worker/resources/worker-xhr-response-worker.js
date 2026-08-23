self.onmessage = function (event) {
  const mode = event.data && event.data.mode;

  function thrownName(callback) {
    try {
      callback();
      return null;
    } catch (error) {
      return error && error.name;
    }
  }

  if (mode === "json") {
    const xhr = new XMLHttpRequest();
    xhr.onload = function () {
      postMessage({
        mode: mode,
        status: xhr.status,
        statusText: xhr.statusText,
        responseType: xhr.responseType,
        over: xhr.response && xhr.response.over,
        updatedAt: xhr.response && xhr.response.updated_at,
        updatedAtType: typeof (xhr.response && xhr.response.updated_at),
        responseTextErrorName: thrownName(function () {
          void xhr.responseText;
        }),
        responseXmlExposed: "responseXML" in xhr,
      });
      close();
    };
    xhr.open("GET", "/wpt/runtime/xhr/json");
    xhr.responseType = "json";
    xhr.send();
    return;
  }

  if (mode === "arraybuffer") {
    const xhr = new XMLHttpRequest();
    xhr.onload = function () {
      postMessage({
        mode: mode,
        status: xhr.status,
        statusText: xhr.statusText,
        responseType: xhr.responseType,
        isArrayBuffer: xhr.response instanceof ArrayBuffer,
        bytes: Array.from(new Uint8Array(xhr.response)),
        responseTextErrorName: thrownName(function () {
          void xhr.responseText;
        }),
        responseXmlExposed: "responseXML" in xhr,
      });
      close();
    };
    xhr.open("GET", "/wpt/runtime/xhr/binary");
    xhr.responseType = "arraybuffer";
    xhr.send();
    return;
  }

  if (mode === "blob") {
    const xhr = new XMLHttpRequest();
    xhr.onload = function () {
      const response = xhr.response;
      response.arrayBuffer().then(function (buffer) {
        postMessage({
          mode: mode,
          status: xhr.status,
          statusText: xhr.statusText,
          responseType: xhr.responseType,
          isBlob: response instanceof Blob,
          size: response.size,
          type: response.type,
          bytes: Array.from(new Uint8Array(buffer)),
          responseTextErrorName: thrownName(function () {
            void xhr.responseText;
          }),
          responseXmlExposed: "responseXML" in xhr,
        });
        close();
      });
    };
    xhr.open("GET", "/wpt/runtime/xhr/binary");
    xhr.responseType = "blob";
    xhr.send();
    return;
  }

  if (mode === "document") {
    const retained = new XMLHttpRequest();
    retained.responseType = "arraybuffer";
    retained.responseType = "document";

    const xhr = new XMLHttpRequest();
    xhr.responseType = "document";
    const initialResponseType = xhr.responseType;
    xhr.onload = function () {
      postMessage({
        mode: mode,
        status: xhr.status,
        statusText: xhr.statusText,
        initialResponseType: initialResponseType,
        retainedResponseType: retained.responseType,
        responseType: xhr.responseType,
        responseIsString: typeof xhr.response === "string",
        responseEqualsText: xhr.response === xhr.responseText,
        responseXmlExposed: "responseXML" in xhr,
        responseText: xhr.responseText,
      });
      close();
    };
    xhr.open("GET", "/wpt/runtime/xhr/html-text");
    xhr.send();
    return;
  }

  if (mode === "state-boundaries") {
    const xhr = new XMLHttpRequest();
    const states = [];
    let loadingErrorName = null;
    let loadingPreservedResponseType = null;
    xhr.onreadystatechange = function () {
      if (xhr.readyState === xhr.LOADING) {
        states.push(xhr.readyState);
        loadingErrorName = thrownName(function () {
          xhr.responseType = "json";
        });
        loadingPreservedResponseType = xhr.responseType;
      }
    };
    xhr.onloadend = function () {
      const doneErrorName = thrownName(function () {
        xhr.responseType = "json";
      });
      postMessage({
        mode: mode,
        states: states,
        loadingErrorName: loadingErrorName,
        loadingPreservedResponseType: loadingPreservedResponseType,
        doneErrorName: doneErrorName,
        donePreservedResponseType: xhr.responseType,
      });
      close();
    };
    xhr.open("GET", "/wpt/runtime/xhr/html-text");
    xhr.responseType = "text";
    xhr.send();
    return;
  }

  if (mode === "invalid-response-type") {
    const xhr = new XMLHttpRequest();
    xhr.responseType = "arraybuffer";
    let errorName = null;
    try {
      xhr.responseType = "invalid";
    } catch (error) {
      errorName = error && error.name;
    }
    postMessage({
      mode: mode,
      errorName: errorName,
      responseType: xhr.responseType,
    });
    close();
    return;
  }

  if (mode === "redirect" || mode === "not-found" || mode === "server-error") {
    const xhr = new XMLHttpRequest();
    const events = [];
    xhr.onload = function () {
      events.push("load");
    };
    xhr.onerror = function () {
      events.push("error");
    };
    xhr.onloadend = function () {
      events.push("loadend");
      postMessage({
        mode: mode,
        status: xhr.status,
        statusText: xhr.statusText,
        responseURL: xhr.responseURL,
        responseText: xhr.responseText,
        events: events,
      });
      close();
    };
    const url =
      mode === "redirect"
        ? "/wpt/runtime/xhr/redirect"
        : mode === "not-found"
          ? "/wpt/runtime/xhr/404"
          : "/wpt/runtime/xhr/500";
    xhr.open("GET", url);
    xhr.send();
    return;
  }

  if (mode === "bad-port") {
    const xhr = new XMLHttpRequest();
    const events = [];
    xhr.onreadystatechange = function () {
      events.push("readystatechange:" + xhr.readyState);
    };
    xhr.onloadstart = function () {
      events.push("loadstart");
    };
    xhr.onload = function () {
      events.push("load");
    };
    xhr.onerror = function () {
      events.push("error");
    };
    xhr.onloadend = function () {
      events.push("loadend");
      postMessage({
        mode: mode,
        readyState: xhr.readyState,
        status: xhr.status,
        statusText: xhr.statusText,
        responseURL: xhr.responseURL,
        responseText: xhr.responseText,
        contentType: xhr.getResponseHeader("Content-Type"),
        allHeaders: xhr.getAllResponseHeaders(),
        events: events,
      });
      close();
    };
    xhr.open("GET", "http://example.test:25/blocked-port");
    xhr.send();
    return;
  }

  if (mode === "timeout") {
    const xhr = new XMLHttpRequest();
    const events = [];
    xhr.onreadystatechange = function () {
      events.push("readystatechange:" + xhr.readyState);
    };
    xhr.onload = function () {
      events.push("load");
    };
    xhr.onerror = function () {
      events.push("error");
    };
    xhr.ontimeout = function () {
      events.push("timeout");
    };
    xhr.onloadend = function () {
      events.push("loadend");
      setTimeout(function () {
        postMessage({
          mode: mode,
          readyState: xhr.readyState,
          status: xhr.status,
          statusText: xhr.statusText,
          responseURL: xhr.responseURL,
          responseText: xhr.responseText,
          contentType: xhr.getResponseHeader("Content-Type"),
          allHeaders: xhr.getAllResponseHeaders(),
          events: events,
        });
        close();
      }, 125);
    };
    xhr.open("GET", "/wpt/runtime/xhr/slow");
    xhr.timeout = 20;
    xhr.send();
    return;
  }

  postMessage({ mode: mode, error: "unknown mode" });
  close();
};

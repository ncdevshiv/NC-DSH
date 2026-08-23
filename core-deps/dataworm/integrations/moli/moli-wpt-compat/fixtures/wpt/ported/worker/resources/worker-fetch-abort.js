self.onmessage = async function (event) {
  const mode = event.data && event.data.mode;
  const targetUrl = event.data && event.data.targetUrl;

  if (mode === "preaborted") {
    const controller = new AbortController();
    controller.abort();
    try {
      await fetch(targetUrl, { signal: controller.signal });
      postMessage({ mode: mode, unexpected: true });
    } catch (error) {
      postMessage({
        mode: mode,
        name: error && error.name,
        message: error && error.message,
        isDomException: error instanceof DOMException,
      });
    }
    close();
    return;
  }

  if (mode === "preaborted-custom-reason") {
    const controller = new AbortController();
    const reason = new Error("worker custom preabort reason");
    controller.abort(reason);
    try {
      await fetch(targetUrl, { signal: controller.signal });
      postMessage({ mode: mode, unexpected: true });
    } catch (error) {
      postMessage({
        mode: mode,
        sameReason: error === reason,
        name: error && error.name,
        message: error && error.message,
        isDomException: error instanceof DOMException,
      });
    }
    close();
    return;
  }

  if (mode === "inflight-abort") {
    const controller = new AbortController();
    const events = [];
    const pending = fetch(targetUrl, { signal: controller.signal });
    setTimeout(function () {
      controller.abort();
    }, 0);
    try {
      await pending;
      events.push("unexpected");
    } catch (error) {
      events.push(
        "error:" +
          (error && error.name) +
          ":" +
          (error instanceof DOMException) +
          ":" +
          (error && error.message),
      );
    }
    await new Promise(function (resolve) {
      setTimeout(resolve, 150);
    });
    postMessage({ mode: mode, events: events });
    close();
    return;
  }

  if (mode === "inflight-custom-reason") {
    const controller = new AbortController();
    const reason = { kind: "worker-custom-inflight-reason" };
    const pending = fetch(targetUrl, { signal: controller.signal });
    setTimeout(function () {
      controller.abort(reason);
    }, 0);
    try {
      await pending;
      postMessage({ mode: mode, unexpected: true });
    } catch (error) {
      postMessage({
        mode: mode,
        sameReason: error === reason,
        kind: error && error.kind,
        isDomException: error instanceof DOMException,
      });
    }
    close();
    return;
  }

  if (mode === "null-signal-override") {
    const controller = new AbortController();
    const request = new Request(targetUrl, { signal: controller.signal });
    controller.abort();
    try {
      const response = await fetch(request, { signal: null });
      postMessage({
        mode: mode,
        status: response.status,
        text: (await response.text()).trim(),
      });
    } catch (error) {
      postMessage({
        mode: mode,
        error: String(error),
        name: error && error.name,
      });
    }
    close();
    return;
  }

  postMessage({ mode: mode, error: "unknown mode" });
  close();
};

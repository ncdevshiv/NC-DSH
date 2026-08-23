self.onmessage = function (event) {
  const mode = event.data && event.data.mode;

  if (mode === "default-abort") {
    const controller = new AbortController();
    const events = [];
    controller.signal.addEventListener("abort", function () {
      events.push("listener");
    });
    controller.signal.onabort = function () {
      events.push("onabort");
    };
    controller.abort();

    let thrown = null;
    try {
      controller.signal.throwIfAborted();
    } catch (error) {
      thrown = {
        name: error && error.name,
        message: error && error.message,
        isDomException: error instanceof DOMException,
      };
    }

    postMessage({
      mode: mode,
      aborted: controller.signal.aborted,
      reasonName: controller.signal.reason && controller.signal.reason.name,
      reasonMessage: controller.signal.reason && controller.signal.reason.message,
      reasonIsDomException: controller.signal.reason instanceof DOMException,
      events: events,
      thrown: thrown,
    });
    close();
    return;
  }

  if (mode === "timeout") {
    const signal = AbortSignal.timeout(0);

    function report() {
      postMessage({
        mode: mode,
        aborted: signal.aborted,
        reasonName: signal.reason && signal.reason.name,
        reasonMessage: signal.reason && signal.reason.message,
        reasonIsDomException: signal.reason instanceof DOMException,
      });
      close();
    }

    if (signal.aborted) {
      report();
      return;
    }

    signal.addEventListener("abort", report, { once: true });
    return;
  }

  if (mode === "any") {
    const first = new AbortController();
    const second = new AbortController();
    const composite = AbortSignal.any([first.signal, second.signal]);
    const events = [];
    composite.addEventListener("abort", function () {
      events.push("listener");
    });
    composite.onabort = function () {
      events.push("onabort");
      postMessage({
        mode: mode,
        aborted: composite.aborted,
        reason: composite.reason,
        events: events,
      });
      close();
    };
    second.abort("second-abort");
    return;
  }

  if (mode === "reentrant-abort") {
    const first = new AbortController();
    const second = new AbortController();
    const events = [];
    second.signal.addEventListener("abort", function () {
      events.push("second-listener");
    });
    first.signal.addEventListener("abort", function () {
      events.push("first-listener");
      second.abort("second-abort");
      events.push("second-state:" + second.signal.aborted + ":" + String(second.signal.reason));
    });
    first.abort("first-abort");
    postMessage({
      mode: mode,
      firstAborted: first.signal.aborted,
      secondAborted: second.signal.aborted,
      secondReason: String(second.signal.reason),
      events: events,
    });
    close();
    return;
  }

  if (mode === "mutate-listeners") {
    const controller = new AbortController();
    const signal = controller.signal;
    let status = "start";

    function original() {
      status = "listener-ran";
      signal.removeEventListener("custom", original);
      signal.addEventListener("custom", function () {
        status += "|late";
      });
      controller.abort("custom-abort");
      status += "|after-abort:" + signal.aborted;
    }

    signal.addEventListener("custom", original);
    signal.dispatchEvent(new Event("custom"));
    postMessage({
      mode: mode,
      status: status,
      aborted: signal.aborted,
      reason: String(signal.reason),
    });
    close();
    return;
  }

  if (mode === "once-listener") {
    const signal = new AbortController().signal;
    let count = 0;
    signal.addEventListener(
      "custom",
      function () {
        count += 1;
      },
      { once: true },
    );
    signal.dispatchEvent(new Event("custom"));
    signal.dispatchEvent(new Event("custom"));
    postMessage({
      mode: mode,
      count: count,
    });
    close();
    return;
  }

  postMessage({ mode: mode, error: "unknown mode" });
  close();
};

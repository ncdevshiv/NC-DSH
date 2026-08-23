onconnect = event => {
  const port = event.ports[0];
  const events = [];
  addEventListener("securitypolicyviolation", event => {
    events.push({
      effectiveDirective: event.effectiveDirective,
      disposition: event.disposition,
      blockedURI: event.blockedURI,
      instance: event instanceof SecurityPolicyViolationEvent,
    });
  });

  const url = new URL("/wpt/runtime/websocket/echo", self.location.href);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  const socket = new WebSocket(url.href);
  socket.onopen = () => socket.send("sharedworker-report-to-websocket");
  socket.onmessage = event => {
    port.postMessage({ data: event.data, events });
    socket.close();
    close();
  };
  socket.onerror = () => {
    port.postMessage({ error: "websocket error", events });
    close();
  };
};

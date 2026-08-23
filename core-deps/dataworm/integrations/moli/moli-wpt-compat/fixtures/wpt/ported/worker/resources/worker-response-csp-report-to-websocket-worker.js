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
socket.onopen = () => socket.send("worker-report-to-websocket");
socket.onmessage = event => {
  postMessage({ data: event.data, events });
  socket.close();
};
socket.onerror = () => {
  postMessage({ error: "websocket error", events });
};

onconnect = event => {
  const port = event.ports[0];
  const url = new URL("/wpt/runtime/websocket/echo", self.location.href);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  const socket = new WebSocket(url.href);
  socket.onopen = () => {
    socket.send("shared-worker-csp-websocket");
  };
  socket.onmessage = event => {
    port.postMessage(event.data);
    socket.close();
    close();
  };
  socket.onerror = () => {
    port.postMessage("websocket-error");
    close();
  };
};

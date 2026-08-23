function calcResponse() {
  return [
    typeof workerStart,
    typeof performance,
    typeof performance.now,
    performance.now(),
    performance.timeOrigin,
  ];
}

self.addEventListener("connect", (event) => {
  const port = event.ports[0];
  port.onmessage = () => {
    port.postMessage(calcResponse());
    port.close();
  };
});

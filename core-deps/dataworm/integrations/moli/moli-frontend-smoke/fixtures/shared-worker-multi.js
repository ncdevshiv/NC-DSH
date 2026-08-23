const ports = [];

addEventListener("connect", (event) => {
  const port = event.ports[0];
  const index = ports.length + 1;
  ports.push(port);
  port.start();
  port.postMessage({ phase: "connected", index, name: self.name });
  for (const current of ports) {
    current.postMessage({ phase: "count", count: ports.length });
  }
  port.addEventListener("message", (message) => {
    if (message.data?.type === "ping") {
      port.postMessage({
        phase: "pong",
        index,
        value: message.data.value,
        clients: ports.length,
      });
      return;
    }
    if (message.data?.type === "shutdown") {
      for (const current of ports) {
        current.postMessage({ phase: "shutdown", clients: ports.length });
        current.close();
      }
      close();
    }
  });
});

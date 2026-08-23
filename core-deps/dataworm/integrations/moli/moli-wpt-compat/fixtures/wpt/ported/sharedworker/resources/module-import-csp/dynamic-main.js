onconnect = async event => {
  const port = event.ports[0];
  try {
    const module = await import("./dependency.js");
    port.postMessage(["dynamic", "unexpected", module.value]);
  } catch (error) {
    port.postMessage(["dynamic", "blocked", error && error.name]);
  }
};

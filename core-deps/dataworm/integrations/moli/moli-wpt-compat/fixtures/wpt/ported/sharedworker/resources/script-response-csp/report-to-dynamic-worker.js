onconnect = async event => {
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

  try {
    const module = await import("./report-to-dynamic-dependency.js");
    port.postMessage({ value: module.value, events });
  } catch (error) {
    port.postMessage({ error: String(error), events });
  }
};

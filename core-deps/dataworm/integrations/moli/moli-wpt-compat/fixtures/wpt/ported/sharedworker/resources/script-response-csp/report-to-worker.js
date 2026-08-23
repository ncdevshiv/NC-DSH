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

  fetch("/wpt/runtime/csp/reporting/target.txt?sharedworker")
    .then(response => response.text())
    .then(text => port.postMessage({ text, events }))
    .catch(error => port.postMessage({ error: String(error), events }));
};

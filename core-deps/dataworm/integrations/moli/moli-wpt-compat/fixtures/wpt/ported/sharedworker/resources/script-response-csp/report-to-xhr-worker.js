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

  const xhr = new XMLHttpRequest();
  xhr.onloadend = () => {
    port.postMessage({
      status: xhr.status,
      responseText: xhr.responseText,
      events,
    });
  };
  xhr.open("GET", "/wpt/runtime/csp/reporting/target.txt?sharedworker-xhr");
  xhr.send();
};

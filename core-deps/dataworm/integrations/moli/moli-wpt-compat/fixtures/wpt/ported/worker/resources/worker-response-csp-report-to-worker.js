const events = [];
addEventListener("securitypolicyviolation", event => {
  events.push({
    effectiveDirective: event.effectiveDirective,
    disposition: event.disposition,
    blockedURI: event.blockedURI,
    instance: event instanceof SecurityPolicyViolationEvent,
  });
});

function xhr_text(url) {
  return new Promise(resolve => {
    const xhr = new XMLHttpRequest();
    xhr.onloadend = () => resolve({
      status: xhr.status,
      responseText: xhr.responseText,
    });
    xhr.open("GET", url);
    xhr.send();
  });
}

try {
  const fetchResponse = await fetch("/wpt/runtime/csp/reporting/target.txt?worker-fetch");
  const xhr = await xhr_text("/wpt/runtime/csp/reporting/target.txt?worker-xhr");
  const module = await import("./worker-response-csp-report-to-dynamic-dependency.js");
  postMessage({
    fetchText: await fetchResponse.text(),
    xhrStatus: xhr.status,
    xhrText: xhr.responseText,
    dynamicValue: module.value,
    events,
  });
} catch (error) {
  postMessage({ error: String(error), events });
}

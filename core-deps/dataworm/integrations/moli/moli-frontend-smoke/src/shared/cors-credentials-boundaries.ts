import { assertFixture, expectNetworkFailure } from "./harness";
import {
  capturePlatformStep,
  errorName,
  fact,
  type CapturePlatformFrame,
  type PlatformBoundaryResult,
  type PlatformFact,
  withEventTimeout,
} from "./platform-boundary";
import type { CaseSpec, SmokeMeta } from "./types";

type CorsScenario = (
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
) => Promise<PlatformFact[]>;

interface CorsPayload {
  body?: string;
  cookieNames?: string[];
  header?: string;
  method?: string;
  origin?: string;
  preflight?: {
    headers: string;
    method: string;
    origin: string;
  } | null;
  referer?: string;
  secFetchMode?: string;
  secFetchSite?: string;
  token?: string;
}

function scenarioRoot(host: HTMLElement, name: string): HTMLElement {
  const root = document.createElement("section");
  root.dataset.corsScenario = name;
  host.append(root);
  return root;
}

function output(root: HTMLElement, name: string, value: string): HTMLOutputElement {
  const element = document.createElement("output");
  element.dataset.corsOutput = name;
  element.textContent = value;
  root.append(element);
  return element;
}

function tokenFor(meta: SmokeMeta, spec: CaseSpec): string {
  return `${meta.framework}-${spec.seed}-${spec.variant}`;
}

function alternateOrigin(): string {
  const url = new URL(location.href);
  url.hostname = url.hostname === "localhost" ? "127.0.0.1" : "localhost";
  return url.origin;
}

function corsUrl(path: string, token: string, query: Record<string, string> = {}): string {
  const url = new URL(path, alternateOrigin());
  url.searchParams.set("token", token);
  for (const [name, value] of Object.entries(query)) {
    url.searchParams.set(name, value);
  }
  return url.href;
}

async function rejectedFetch(
  input: RequestInfo | URL,
  label: string,
  init?: RequestInit,
): Promise<string> {
  const url = input instanceof Request ? input.url : String(input);
  expectNetworkFailure({ label, url, type: "Fetch", canceled: false });
  try {
    await fetch(input, init);
    return "resolved";
  } catch (error: unknown) {
    return errorName(error);
  }
}

async function simpleAllowDeny(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = tokenFor(meta, spec);
  const allowedUrl = corsUrl("/support/cors/allow", token);
  const response = await fetch(allowedUrl);
  const payload = (await response.json()) as CorsPayload;
  assertFixture(response.type === "cors", "allowed cross-origin fetch exposed a CORS response");
  assertFixture(payload.origin === location.origin, "server observed the document Origin header");
  assertFixture(payload.method === "GET", "simple CORS request retained GET");
  output(root, "allow", `${response.status}\n${response.type}\n${payload.method}\n${payload.origin === location.origin}`);
  await capturePlatformStep(host, capture, "platform-1", "cors-simple-allowed", [
    response.status,
    response.type,
    payload.method,
    payload.origin === location.origin,
  ]);

  const deniedUrl = corsUrl("/support/cors/deny", token);
  const denied = await rejectedFetch(deniedUrl, "cors-simple-denied");
  assertFixture(denied === "TypeError", "missing allow-origin rejected fetch with TypeError");
  output(root, "deny", denied);
  await capturePlatformStep(host, capture, "platform-2", "cors-simple-denied", [denied]);

  return [
    fact("allowed-status", response.status),
    fact("allowed-type", response.type),
    fact("origin-sent", payload.origin === location.origin),
    fact("denied-error", denied),
  ];
}

async function exposedHeaderFiltering(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = tokenFor(meta, spec);
  const response = await fetch(corsUrl("/support/cors/exposed", token));
  const visible = response.headers.get("x-smoke-visible");
  const hidden = response.headers.get("x-smoke-hidden");
  const forbidden = response.headers.get("x-content-type-options");
  assertFixture(visible === `visible-${token}`, "explicitly exposed response header is readable");
  assertFixture(hidden === null, "unexposed custom response header is hidden");
  assertFixture(forbidden === null, "unexposed fixture security header is hidden");
  output(root, "header-get", `${visible}\n${hidden}\n${forbidden}\n${response.headers.get("content-type")}`);
  await capturePlatformStep(host, capture, "platform-1", "cors-header-get", [
    visible,
    hidden,
    forbidden,
    response.headers.get("content-type"),
  ]);

  const names = Array.from(response.headers.keys()).sort();
  assertFixture(names.includes("x-smoke-visible"), "Headers iterator includes exposed header");
  assertFixture(!names.includes("x-smoke-hidden"), "Headers iterator excludes hidden header");
  assertFixture(!names.includes("access-control-allow-origin"), "CORS protocol header is filtered");
  const text = await response.text();
  output(root, "header-iterator", `${names.join("|")}\n${text}`);
  await capturePlatformStep(host, capture, "platform-2", "cors-header-iterator", [
    names.join(","),
    text,
  ]);

  return [
    fact("visible", visible),
    fact("hidden", hidden),
    fact("protocol-hidden", !names.includes("access-control-allow-origin")),
    fact("names", names.join("|")),
    fact("body", text),
  ];
}

async function credentialsResponseGuard(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = tokenFor(meta, spec);
  const url = corsUrl("/support/cors/credentials", token, { set: "1" });
  const request = new Request(url, { credentials: "include", mode: "cors" });
  assertFixture(request.credentials === "include", "Request retained credential mode");
  output(root, "credentials-request", `${request.mode}\n${request.credentials}\n${new URL(request.url).hostname}`);
  await capturePlatformStep(host, capture, "platform-1", "cors-credentials-request", [
    request.mode,
    request.credentials,
    new URL(request.url).hostname,
  ]);

  const response = await fetch(request);
  const payload = (await response.json()) as CorsPayload;
  const setCookie = response.headers.get("set-cookie");
  assertFixture(response.type === "cors", "credential response passed CORS checks");
  assertFixture(setCookie === null, "Set-Cookie remains a forbidden response header");
  assertFixture(payload.origin === location.origin, "credential request sent Origin");
  output(
    root,
    "credentials-response",
    `${response.status}\n${response.type}\n${setCookie}\n${payload.origin === location.origin}\n${payload.cookieNames?.length ?? -1}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "cors-credentials-response", [
    response.status,
    response.type,
    setCookie,
    payload.origin === location.origin,
    payload.cookieNames?.length,
  ]);

  return [
    fact("mode", request.mode),
    fact("credentials", request.credentials),
    fact("response-type", response.type),
    fact("set-cookie-hidden", setCookie === null),
    fact("server-cookie-count", payload.cookieNames?.length),
  ];
}

async function wildcardCredentialsRejected(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = tokenFor(meta, spec);
  const wildcard = corsUrl("/support/cors/wildcard", token);
  const anonymous = await fetch(wildcard);
  const text = await anonymous.text();
  assertFixture(text === "cors-wildcard", "wildcard allowed a non-credentialed request");
  output(root, "wildcard-anonymous", `${anonymous.status}\n${anonymous.type}\n${text}`);
  await capturePlatformStep(host, capture, "platform-1", "cors-wildcard-anonymous", [
    anonymous.status,
    anonymous.type,
    text,
  ]);

  const credentialUrl = corsUrl("/support/cors/credentials", token, { wildcard: "1" });
  const denied = await rejectedFetch(credentialUrl, "cors-wildcard-credentials", {
    credentials: "include",
  });
  assertFixture(denied === "TypeError", "wildcard credential response was rejected");
  output(root, "wildcard-credentials", denied);
  await capturePlatformStep(host, capture, "platform-2", "cors-wildcard-credentials", [denied]);

  return [
    fact("anonymous", text),
    fact("anonymous-type", anonymous.type),
    fact("credential-error", denied),
  ];
}

async function allowedPreflightPut(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = tokenFor(meta, spec);
  const url = corsUrl("/support/cors/preflight/allow", token);
  const request = new Request(url, {
    method: "PUT",
    headers: { "Content-Type": "application/json", "X-Smoke-Token": token },
    body: JSON.stringify({ framework: meta.framework, variant: spec.variant }),
  });
  output(root, "preflight-request", `${request.method}\n${request.headers.get("content-type")}\n${request.headers.get("x-smoke-token")}`);
  await capturePlatformStep(host, capture, "platform-1", "cors-preflight-request", [
    request.method,
    request.headers.get("content-type"),
    request.headers.get("x-smoke-token"),
  ]);

  const response = await fetch(request);
  const payload = (await response.json()) as CorsPayload;
  assertFixture(payload.method === "PUT", "actual request retained PUT after preflight");
  assertFixture(payload.header === token, "actual request retained custom header");
  assertFixture(payload.preflight?.method === "PUT", "server observed PUT preflight method");
  assertFixture(payload.preflight.headers.includes("x-smoke-token"), "server observed custom preflight header");
  assertFixture(payload.preflight.origin === location.origin, "preflight sent document Origin");
  output(
    root,
    "preflight-response",
    `${response.status}\n${response.headers.get("x-smoke-actual")}\n${payload.method}\n${payload.header}\n${payload.preflight.method}\n${payload.preflight.headers}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "cors-preflight-response", [
    response.status,
    response.headers.get("x-smoke-actual"),
    payload.method,
    payload.header,
    payload.preflight.method,
    payload.preflight.headers,
  ]);

  return [
    fact("method", payload.method),
    fact("header", payload.header),
    fact("preflight-method", payload.preflight.method),
    fact("preflight-headers", payload.preflight.headers),
    fact("body", payload.body),
  ];
}

async function deniedPreflightMethod(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = tokenFor(meta, spec);
  const url = corsUrl("/support/cors/preflight/deny-method", token);
  const request = new Request(url, {
    method: "PUT",
    headers: { "Content-Type": "application/json", "X-Smoke-Token": token },
    body: `method:${token}`,
  });
  output(root, "deny-method-request", `${request.method}\n${request.bodyUsed}\n${request.headers.get("x-smoke-token")}`);
  await capturePlatformStep(host, capture, "platform-1", "cors-deny-method-request", [
    request.method,
    request.bodyUsed,
    request.headers.get("x-smoke-token"),
  ]);

  const denied = await rejectedFetch(request, "cors-preflight-method-denied");
  assertFixture(denied === "TypeError", "disallowed preflight method rejected fetch");
  assertFixture(request.bodyUsed, "fetch disturbed the request body before preflight rejection");
  output(root, "deny-method-result", `${denied}\n${request.bodyUsed}`);
  await capturePlatformStep(host, capture, "platform-2", "cors-deny-method-result", [
    denied,
    request.bodyUsed,
  ]);

  return [fact("error", denied), fact("body-used", request.bodyUsed), fact("method", request.method)];
}

async function deniedPreflightHeader(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = tokenFor(meta, spec);
  const url = corsUrl("/support/cors/preflight/deny-header", token);
  const headers = new Headers([
    ["X-Smoke-Token", token],
    ["Content-Type", "application/json"],
  ]);
  output(root, "deny-header-request", Array.from(headers).map(([name, value]) => `${name}:${value}`).join("\n"));
  await capturePlatformStep(host, capture, "platform-1", "cors-deny-header-request", [
    Array.from(headers.keys()).join(","),
    headers.get("x-smoke-token"),
  ]);

  const denied = await rejectedFetch(url, "cors-preflight-header-denied", {
    method: "POST",
    headers,
    body: `header:${token}`,
  });
  assertFixture(denied === "TypeError", "disallowed preflight header rejected fetch");
  output(root, "deny-header-result", denied);
  await capturePlatformStep(host, capture, "platform-2", "cors-deny-header-result", [denied]);

  return [
    fact("error", denied),
    fact("header-names", Array.from(headers.keys()).join("|")),
    fact("token", headers.get("x-smoke-token")),
  ];
}

async function xhrPreflightLifecycle(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = tokenFor(meta, spec);
  const url = corsUrl("/support/cors/preflight/allow", token);
  const request = new XMLHttpRequest();
  const events: string[] = [];
  request.addEventListener("loadstart", () => events.push("loadstart"));
  request.addEventListener("readystatechange", () => events.push(`state:${request.readyState}`));
  request.addEventListener("progress", () => events.push("progress"));
  request.addEventListener("load", () => events.push("load"));
  request.addEventListener("loadend", () => events.push("loadend"));
  request.open("POST", url);
  request.setRequestHeader("Content-Type", "application/json");
  request.setRequestHeader("X-Smoke-Token", token);
  output(root, "xhr-open", `${request.readyState}\n${request.status}`);
  await capturePlatformStep(host, capture, "platform-1", "cors-xhr-open", [
    request.readyState,
    request.status,
    events.join(","),
  ]);

  const completed = withEventTimeout(
    new Promise<void>((resolve, reject) => {
      request.addEventListener("loadend", () => resolve(), { once: true });
      request.addEventListener("error", () => reject(new Error("CORS XHR emitted error")), {
        once: true,
      });
    }),
    "CORS XHR loadend",
  );
  request.send(JSON.stringify({ framework: meta.framework, seed: spec.seed }));
  await completed;
  const payload = JSON.parse(request.responseText) as CorsPayload;
  assertFixture(request.status === 200, "CORS XHR returned HTTP 200");
  assertFixture(request.getResponseHeader("x-smoke-actual") === "allow", "XHR exposed allowed response header");
  assertFixture(request.getResponseHeader("x-content-type-options") === null, "XHR hid unexposed response header");
  assertFixture(events[0] === "state:1", "XHR recorded OPENED before loadstart");
  assertFixture(events.at(-1) === "loadend", "XHR lifecycle ended with loadend");
  assertFixture(events.includes("state:4") && events.includes("load"), "XHR published DONE and load");
  output(root, "xhr-complete", `${request.status}\n${events.join("|")}\n${payload.preflight?.method}\n${payload.header}`);
  await capturePlatformStep(host, capture, "platform-2", "cors-xhr-complete", [
    request.status,
    events.join(","),
    payload.preflight?.method,
    payload.header,
  ]);

  return [
    fact("status", request.status),
    fact("events", events.join("|")),
    fact("preflight-method", payload.preflight?.method),
    fact("header", payload.header),
  ];
}

async function redirectAfterPreflight(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = tokenFor(meta, spec);
  const initialUrl = corsUrl("/support/cors/preflight/redirect", token);
  const request = new Request(initialUrl, {
    method: "PUT",
    headers: { "Content-Type": "application/json", "X-Smoke-Token": token },
    body: `redirect:${meta.framework}:${spec.variant}`,
  });
  output(root, "redirect-request", `${request.method}\n${new URL(request.url).pathname}\n${request.redirect}`);
  await capturePlatformStep(host, capture, "platform-1", "cors-redirect-request", [
    request.method,
    new URL(request.url).pathname,
    request.redirect,
  ]);

  const response = await fetch(request);
  const payload = (await response.json()) as CorsPayload;
  assertFixture(response.redirected, "CORS fetch exposed redirected state");
  assertFixture(new URL(response.url).pathname === "/support/cors/preflight/final", "CORS redirect reached final route");
  assertFixture(payload.method === "PUT", "307 preserved method across CORS redirect");
  assertFixture(payload.header === token, "307 preserved custom header across CORS redirect");
  assertFixture(payload.body === `redirect:${meta.framework}:${spec.variant}`, "307 preserved body across CORS redirect");
  output(
    root,
    "redirect-response",
    `${response.status}\n${response.redirected}\n${new URL(response.url).pathname}\n${payload.method}\n${payload.header}\n${payload.body}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "cors-redirect-response", [
    response.status,
    response.redirected,
    new URL(response.url).pathname,
    payload.method,
    payload.header,
    payload.body,
  ]);

  return [
    fact("redirected", response.redirected),
    fact("final-path", new URL(response.url).pathname),
    fact("method", payload.method),
    fact("header", payload.header),
    fact("body", payload.body),
  ];
}

async function opaqueAndRequestMetadata(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = tokenFor(meta, spec);
  const opaqueUrl = corsUrl("/support/cors/no-cors", token);
  expectNetworkFailure({
    label: "cors-opaque-body-hidden",
    url: opaqueUrl,
    type: "Fetch",
    canceled: true,
  });
  const opaque = await fetch(opaqueUrl, { mode: "no-cors" });
  const opaqueText = "";
  assertFixture(opaque.type === "opaque", "no-cors fetch returned an opaque response");
  assertFixture(opaque.status === 0 && !opaque.ok, "opaque response hid network status");
  assertFixture(opaque.url === "" && opaque.body === null, "opaque response hid URL and body");
  assertFixture(Array.from(opaque.headers).length === 0, "opaque response hid all headers");
  output(root, "opaque", `${opaque.type}\n${opaque.status}\n${opaque.ok}\n${opaque.url}\n${opaqueText}`);
  await capturePlatformStep(host, capture, "platform-1", "cors-opaque-response", [
    opaque.type,
    opaque.status,
    opaque.ok,
    opaque.url,
    opaqueText,
    Array.from(opaque.headers).length,
  ]);

  const response = await fetch(corsUrl("/support/cors/metadata", token));
  const payload = (await response.json()) as CorsPayload;
  assertFixture(payload.origin === location.origin, "CORS metadata request sent Origin");
  assertFixture(payload.secFetchMode === "cors", "CORS request sent Sec-Fetch-Mode cors");
  assertFixture(payload.secFetchSite === "cross-site", "alternate host request was cross-site");
  assertFixture(payload.referer === `${location.origin}/`, "cross-site referrer was reduced to origin");
  output(
    root,
    "metadata",
    `${payload.origin === location.origin}\n${payload.referer === `${location.origin}/`}\n${payload.secFetchMode}\n${payload.secFetchSite}\n${response.headers.get("x-smoke-metadata")}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "cors-request-metadata", [
    payload.origin === location.origin,
    payload.referer === `${location.origin}/`,
    payload.secFetchMode,
    payload.secFetchSite,
    response.headers.get("x-smoke-metadata"),
  ]);

  return [
    fact("opaque", `${opaque.type}|${opaque.status}|${opaque.ok}`),
    fact("opaque-body", opaqueText),
    fact("origin", payload.origin === location.origin),
    fact("referer-origin", payload.referer === `${location.origin}/`),
    fact("fetch-metadata", `${payload.secFetchMode}|${payload.secFetchSite}`),
  ];
}

const SCENARIOS: Record<string, CorsScenario> = {
  "simple-allow-deny": simpleAllowDeny,
  "exposed-header-filtering": exposedHeaderFiltering,
  "credentials-response-guard": credentialsResponseGuard,
  "wildcard-credentials-rejected": wildcardCredentialsRejected,
  "allowed-preflight-put": allowedPreflightPut,
  "denied-preflight-method": deniedPreflightMethod,
  "denied-preflight-header": deniedPreflightHeader,
  "xhr-preflight-lifecycle": xhrPreflightLifecycle,
  "redirect-after-preflight": redirectAfterPreflight,
  "opaque-and-request-metadata": opaqueAndRequestMetadata,
};

export async function runCorsCredentialsBoundaryCase(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformBoundaryResult> {
  const scenario = SCENARIOS[spec.slug];
  assertFixture(scenario, `missing CORS/credentials scenario ${spec.slug}`);
  const facts = await scenario(host, meta, spec, capture);
  return { status: "ready", facts };
}

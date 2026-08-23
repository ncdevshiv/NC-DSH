import { assertFixture } from "./harness";
import type { CaseSpec, SmokeMeta } from "./types";

export interface BrowsingContextBoundaryResult {
  status: "ready";
  facts: Array<{
    name: string;
    value: string;
  }>;
}

type BoundaryFact = BrowsingContextBoundaryResult["facts"][number];
type CaptureBoundary = (name: "boundary-1" | "boundary-2") => Promise<void>;

interface BoundaryMessage {
  source: "moli-boundary-frame";
  scenario: string;
  phase: string;
  type: string;
  label?: string;
}

const FRAME_EVENT_TIMEOUT_MS = 20_000;

function fact(name: string, value: unknown): BoundaryFact {
  return { name, value: String(value) };
}

function appendBoundaryLog(host: HTMLElement, name: string, value: string): void {
  let log = host.querySelector("[data-boundary-log]");
  if (!log) {
    log = document.createElement("ol");
    log.setAttribute("data-boundary-log", "");
    host.prepend(log);
  }
  const item = document.createElement("li");
  item.dataset.boundary = name;
  item.textContent = `${name}:${value}`;
  log.append(item);
  host.dataset.lastBoundary = name;
}

async function captureBoundary(
  host: HTMLElement,
  capture: CaptureBoundary,
  name: "boundary-1" | "boundary-2",
  value: string,
): Promise<void> {
  appendBoundaryLog(host, name, value);
  await capture(name);
}

function isBoundaryMessage(value: unknown): value is BoundaryMessage {
  if (!value || typeof value !== "object") {
    return false;
  }
  const candidate = value as Partial<BoundaryMessage>;
  return (
    candidate.source === "moli-boundary-frame" &&
    typeof candidate.scenario === "string" &&
    typeof candidate.phase === "string" &&
    typeof candidate.type === "string"
  );
}

function waitForFrameMessage(
  frame: HTMLIFrameElement,
  label: string,
  predicate: (message: BoundaryMessage, event: MessageEvent) => boolean,
): Promise<{ message: BoundaryMessage; event: MessageEvent }> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      removeEventListener("message", onMessage);
      reject(new Error(`timed out waiting for ${label}`));
    }, FRAME_EVENT_TIMEOUT_MS);
    const onMessage = (event: MessageEvent): void => {
      if (event.source !== frame.contentWindow || !isBoundaryMessage(event.data)) {
        return;
      }
      if (!predicate(event.data, event)) {
        return;
      }
      clearTimeout(timer);
      removeEventListener("message", onMessage);
      resolve({ message: event.data, event });
    };
    addEventListener("message", onMessage);
  });
}

function waitForFrameLoad(frame: HTMLIFrameElement, label: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      reject(new Error(`timed out waiting for ${label} load`));
    }, FRAME_EVENT_TIMEOUT_MS);
    frame.addEventListener(
      "load",
      () => {
        clearTimeout(timer);
        resolve();
      },
      { once: true },
    );
    frame.addEventListener(
      "error",
      () => {
        clearTimeout(timer);
        reject(new Error(`${label} failed to load`));
      },
      { once: true },
    );
  });
}

async function appendBoundaryFrame(
  host: HTMLElement,
  options: {
    title: string;
    scenario: string;
    phase: string;
    src?: string;
    srcdoc?: string;
    sandbox?: string;
  },
): Promise<{ frame: HTMLIFrameElement; ready: BoundaryMessage; origin: string }> {
  const frame = document.createElement("iframe");
  frame.title = options.title;
  if (options.sandbox !== undefined) {
    frame.setAttribute("sandbox", options.sandbox);
  }
  const ready = waitForFrameMessage(
    frame,
    `${options.title} ready`,
    (message) =>
      message.type === "ready" &&
      message.scenario === options.scenario &&
      message.phase === options.phase,
  );
  const loaded = waitForFrameLoad(frame, options.title);
  if (options.srcdoc !== undefined) {
    frame.srcdoc = options.srcdoc;
  } else {
    assertFixture(options.src, `${options.title} has a source URL`);
    frame.src = options.src;
  }
  host.append(frame);
  const [{ message, event }] = await Promise.all([ready, loaded]);
  return { frame, ready: message, origin: event.origin };
}

async function navigateBoundaryFrame(
  frame: HTMLIFrameElement,
  options: {
    title: string;
    scenario: string;
    phase: string;
    src?: string;
    srcdoc?: string;
  },
): Promise<{ message: BoundaryMessage; origin: string }> {
  const ready = waitForFrameMessage(
    frame,
    `${options.title} ready`,
    (message) =>
      message.type === "ready" &&
      message.scenario === options.scenario &&
      message.phase === options.phase,
  );
  const loaded = waitForFrameLoad(frame, options.title);
  if (options.srcdoc !== undefined) {
    frame.srcdoc = options.srcdoc;
  } else {
    assertFixture(options.src, `${options.title} has a source URL`);
    frame.src = options.src;
  }
  const [{ message, event }] = await Promise.all([ready, loaded]);
  return { message, origin: event.origin };
}

function postBoundaryCommand(
  frame: HTMLIFrameElement,
  command: string,
  values: Record<string, unknown> = {},
  transfer: Transferable[] = [],
): void {
  const target = frame.contentWindow;
  assertFixture(target, `${frame.title} exposes a WindowProxy`);
  target.postMessage(
    { source: "moli-boundary-parent", command, ...values },
    "*",
    transfer,
  );
}

async function sendBoundaryCommand(
  frame: HTMLIFrameElement,
  command: string,
  responseType: string,
  values: Record<string, unknown> = {},
): Promise<BoundaryMessage> {
  const response = waitForFrameMessage(
    frame,
    `${frame.title} ${responseType}`,
    (message) => message.type === responseType,
  );
  postBoundaryCommand(frame, command, values);
  return (await response).message;
}

function messageSrcdoc(scenario: string, phase: string): string {
  const scenarioLiteral = JSON.stringify(scenario);
  const phaseLiteral = JSON.stringify(phase);
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><title>srcdoc boundary</title></head><body><main id="srcdoc-root"><h2>${scenario}</h2><p data-phase="${phase}">${phase}</p></main><script>(() => { const scenario = ${scenarioLiteral}; const phase = ${phaseLiteral}; parent.postMessage({ source: "moli-boundary-frame", scenario, phase, type: "ready" }, "*"); })();<\/script></body></html>`;
}

function realmSrcdoc(elementName: string, generation: string): string {
  const elementLiteral = JSON.stringify(elementName);
  const generationLiteral = JSON.stringify(generation);
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><title>realm ${generation}</title></head><body><main><${elementName} id="realm-probe"></${elementName}></main><script>(() => { const elementName = ${elementLiteral}; const generation = ${generationLiteral}; customElements.define(elementName, class extends HTMLElement { constructor() { super(); const root = this.attachShadow({ mode: "open" }); const output = document.createElement("output"); output.textContent = "realm:" + generation; root.append(output); } connectedCallback() { this.dataset.generation = generation; } }); parent.postMessage({ source: "moli-boundary-frame", scenario: "srcdoc-realm", phase: generation, type: "ready" }, "*"); })();<\/script></body></html>`;
}

async function crossOriginMessageChannel(
  host: HTMLElement,
  meta: SmokeMeta,
  capture: CaptureBoundary,
): Promise<BoundaryFact[]> {
  const { frame, origin } = await appendBoundaryFrame(host, {
    title: "cross-origin message channel",
    scenario: "message-channel",
    phase: "frame-ready",
    src: "/support/alternate-origin-frame?scenario=message-channel&phase=frame-ready",
  });
  assertFixture(origin !== location.origin, "alternate frame has a distinct origin");
  await captureBoundary(host, capture, "boundary-1", "alternate-frame-ready");

  const channel = new MessageChannel();
  const portReady = new Promise<MessageEvent>((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error("timed out waiting for transferred port readiness")),
      FRAME_EVENT_TIMEOUT_MS,
    );
    channel.port1.addEventListener(
      "message",
      (event) => {
        if (event.data?.type !== "port-ready") {
          return;
        }
        clearTimeout(timer);
        resolve(event);
      },
      { once: true },
    );
    channel.port1.start();
  });
  postBoundaryCommand(frame, "bind-port", {}, [channel.port2]);
  await portReady;
  const pong = new Promise<MessageEvent>((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error("timed out waiting for transferred port pong")),
      FRAME_EVENT_TIMEOUT_MS,
    );
    channel.port1.addEventListener(
      "message",
      (event) => {
        if (event.data?.type !== "pong") {
          return;
        }
        clearTimeout(timer);
        resolve(event);
      },
      { once: true },
    );
  });
  channel.port1.postMessage({ label: `${meta.framework}-port` });
  const pongEvent = await pong;
  channel.port1.close();
  appendBoundaryLog(host, "port-result", String(pongEvent.data.label));
  await captureBoundary(host, capture, "boundary-2", "message-port-roundtrip");

  return [
    fact("origin-boundary", "cross-origin"),
    fact("port-transfer", "ready"),
    fact("port-pong", pongEvent.data.label),
  ];
}

async function crossOriginFrameRenavigation(
  host: HTMLElement,
  meta: SmokeMeta,
  capture: CaptureBoundary,
): Promise<BoundaryFact[]> {
  const { frame } = await appendBoundaryFrame(host, {
    title: "cross-origin renavigation",
    scenario: "renavigation",
    phase: "alpha",
    src: "/support/alternate-origin-frame?scenario=renavigation&phase=alpha",
  });
  const firstWindow = frame.contentWindow;
  await captureBoundary(host, capture, "boundary-1", "alpha-document");
  await navigateBoundaryFrame(frame, {
    title: "cross-origin renavigation beta",
    scenario: "renavigation",
    phase: "beta",
    src: "/support/alternate-origin-frame?scenario=renavigation&phase=beta",
  });
  assertFixture(frame.contentWindow === firstWindow, "renavigation preserves the WindowProxy");
  await captureBoundary(host, capture, "boundary-2", "beta-document");
  const rendered = await sendBoundaryCommand(frame, "render", "rendered", {
    label: `${meta.framework}-after-navigation`,
  });
  return [
    fact("window-proxy-stable", frame.contentWindow === firstWindow),
    fact("navigation-phase", "alpha>beta"),
    fact("child-render", rendered.label),
  ];
}

async function sameToCrossOriginTransition(
  host: HTMLElement,
  meta: SmokeMeta,
  capture: CaptureBoundary,
): Promise<BoundaryFact[]> {
  const { frame } = await appendBoundaryFrame(host, {
    title: "same-to-cross origin transition",
    scenario: "origin-transition",
    phase: "same",
    src: "/support/boundary-frame.html?scenario=origin-transition&phase=same",
  });
  assertFixture(frame.contentDocument, "same-origin frame exposes contentDocument");
  frame.contentDocument.body.dataset.parentRead = "same-origin";
  await captureBoundary(host, capture, "boundary-1", "same-origin-readable");

  await navigateBoundaryFrame(frame, {
    title: "same-to-cross origin transition alternate",
    scenario: "origin-transition",
    phase: "alternate",
    src: "/support/alternate-origin-frame?scenario=origin-transition&phase=alternate",
  });
  let accessResult = "allowed";
  try {
    void frame.contentWindow?.document.body;
  } catch (error) {
    accessResult = error instanceof DOMException ? error.name : "Error";
  }
  assertFixture(frame.contentDocument === null, "cross-origin frame hides contentDocument");
  assertFixture(accessResult === "SecurityError", "cross-origin document access throws SecurityError");
  await captureBoundary(host, capture, "boundary-2", "cross-origin-protected");
  const rendered = await sendBoundaryCommand(frame, "render", "rendered", {
    label: `${meta.framework}-cross-origin`,
  });
  return [
    fact("same-origin-document", "readable"),
    fact("cross-origin-document", "null"),
    fact("cross-origin-access", accessResult),
    fact("cross-origin-render", rendered.label),
  ];
}

async function nestedCrossOriginFrameChain(
  host: HTMLElement,
  capture: CaptureBoundary,
): Promise<BoundaryFact[]> {
  const { frame } = await appendBoundaryFrame(host, {
    title: "nested cross-origin host",
    scenario: "nested-host",
    phase: "outer-ready",
    src: "/support/alternate-origin-frame?scenario=nested-host&phase=outer-ready",
  });
  await captureBoundary(host, capture, "boundary-1", "outer-ready");
  const nestedReady = waitForFrameMessage(
    frame,
    "nested frame relay",
    (message) => message.type === "nested-ready",
  );
  postBoundaryCommand(frame, "spawn-nested");
  await nestedReady;
  await captureBoundary(host, capture, "boundary-2", "nested-ready");
  const rendered = await sendBoundaryCommand(frame, "render", "rendered", {
    label: "outer-after-nested",
  });
  return [
    fact("frame-depth", 2),
    fact("origin-chain", "ipv4>localhost>ipv4"),
    fact("outer-render", rendered.label),
  ];
}

async function sandboxedOpaqueOriginFrame(
  host: HTMLElement,
  capture: CaptureBoundary,
): Promise<BoundaryFact[]> {
  const { frame, origin } = await appendBoundaryFrame(host, {
    title: "sandboxed opaque origin",
    scenario: "sandbox-origin",
    phase: "opaque",
    srcdoc: messageSrcdoc("sandbox-origin", "opaque"),
    sandbox: "allow-scripts",
  });
  assertFixture(origin === "null", "sandboxed srcdoc reports an opaque origin");
  assertFixture(frame.contentDocument === null, "opaque srcdoc hides contentDocument");
  await captureBoundary(host, capture, "boundary-1", "opaque-origin");

  frame.removeAttribute("sandbox");
  const second = await navigateBoundaryFrame(frame, {
    title: "unsandboxed srcdoc",
    scenario: "sandbox-origin",
    phase: "same-origin",
    srcdoc: messageSrcdoc("sandbox-origin", "same-origin"),
  });
  assertFixture(second.origin === location.origin, "unsandboxed srcdoc inherits parent origin");
  const restoredDocument = frame.contentWindow?.document;
  assertFixture(restoredDocument, "unsandboxed srcdoc exposes its Document through WindowProxy");
  await captureBoundary(host, capture, "boundary-2", "same-origin-restored");
  restoredDocument.body.dataset.parentWrite = "ready";
  return [
    fact("opaque-message-origin", origin),
    fact("restored-message-origin", "same-origin"),
    fact("restored-document", true),
  ];
}

async function iframeSrcdocRealmReplacement(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CaptureBoundary,
): Promise<BoundaryFact[]> {
  const elementName = `x-realm-${meta.framework}-${spec.seed}`;
  const { frame } = await appendBoundaryFrame(host, {
    title: "srcdoc realm generation one",
    scenario: "srcdoc-realm",
    phase: "one",
    srcdoc: realmSrcdoc(elementName, "one"),
  });
  const firstDocument = frame.contentDocument;
  const windowProxy = frame.contentWindow;
  assertFixture(firstDocument, "first srcdoc document exists");
  await captureBoundary(host, capture, "boundary-1", "realm-one");

  await navigateBoundaryFrame(frame, {
    title: "srcdoc realm generation two",
    scenario: "srcdoc-realm",
    phase: "two",
    srcdoc: realmSrcdoc(elementName, "two"),
  });
  const secondDocument = frame.contentDocument;
  assertFixture(secondDocument, "second srcdoc document exists");
  assertFixture(secondDocument !== firstDocument, "srcdoc navigation replaces the Document");
  assertFixture(frame.contentWindow === windowProxy, "srcdoc navigation preserves WindowProxy");
  await captureBoundary(host, capture, "boundary-2", "realm-two");
  const probe = secondDocument.querySelector("#realm-probe");
  assertFixture(probe, "replacement realm contains its custom element");
  probe.setAttribute("data-parent-update", "ready");
  return [
    fact("document-replaced", secondDocument !== firstDocument),
    fact("window-proxy-stable", frame.contentWindow === windowProxy),
    fact("registry-generation", probe.getAttribute("data-generation")),
    fact("shadow-text", probe.shadowRoot?.textContent ?? "missing"),
  ];
}

async function customElementAdoptAcrossDocuments(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CaptureBoundary,
): Promise<BoundaryFact[]> {
  const lifecycle: string[] = [];
  const elementName = `x-adopt-${meta.framework}-${spec.seed}`;
  class AdoptableElement extends HTMLElement {
    constructor() {
      super();
      lifecycle.push("constructor");
      const root = this.attachShadow({ mode: "open" });
      root.innerHTML = '<slot></slot><output data-owner="top">top</output>';
    }

    connectedCallback(): void {
      lifecycle.push("connected");
    }

    disconnectedCallback(): void {
      lifecycle.push("disconnected");
    }

    adoptedCallback(): void {
      lifecycle.push("adopted");
      const output = this.shadowRoot?.querySelector("output");
      if (output) {
        output.textContent = "adopted";
      }
    }
  }
  customElements.define(elementName, AdoptableElement);
  const child = await appendBoundaryFrame(host, {
    title: "custom element adoption document",
    scenario: "adoption-document",
    phase: "ready",
    srcdoc: messageSrcdoc("adoption-document", "ready"),
  });
  const childDocument = child.frame.contentDocument;
  assertFixture(childDocument, "adoption child document exists");
  const element = document.createElement(elementName);
  element.id = "adoptable-element";
  element.textContent = `${meta.framework} payload`;
  host.append(element);
  await captureBoundary(host, capture, "boundary-1", "connected-top");

  childDocument.adoptNode(element);
  childDocument.body.append(element);
  assertFixture(element.ownerDocument === childDocument, "element adopted into child document");
  await captureBoundary(host, capture, "boundary-2", "connected-child");

  document.adoptNode(element);
  host.append(element);
  element.setAttribute("data-final-owner", "top");
  const output = element.shadowRoot?.querySelector("output");
  if (output) {
    output.setAttribute("data-owner", "top-again");
    output.textContent = "top-again";
  }
  return [
    fact("final-owner", element.ownerDocument === document ? "top" : "child"),
    fact("lifecycle", lifecycle.join("|")),
    fact("shadow-owner", output?.getAttribute("data-owner") ?? "missing"),
  ];
}

async function detachedCustomElementUpgrade(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CaptureBoundary,
): Promise<BoundaryFact[]> {
  const lifecycle: string[] = [];
  const elementName = `x-detached-${meta.framework}-${spec.seed}`;
  const container = document.createElement("section");
  container.id = "detached-upgrade-container";
  container.innerHTML = `<${elementName} id="detached-upgrade" status="waiting"><span>detached payload</span></${elementName}>`;
  const element = container.firstElementChild;
  assertFixture(element instanceof HTMLElement, "detached upgrade target exists");

  class DetachedUpgradeElement extends HTMLElement {
    static get observedAttributes(): string[] {
      return ["status"];
    }

    constructor() {
      super();
      lifecycle.push("constructor");
      const root = this.attachShadow({ mode: "open" });
      root.innerHTML = '<slot></slot><output id="upgrade-status">constructed</output>';
    }

    connectedCallback(): void {
      lifecycle.push("connected");
    }

    attributeChangedCallback(_name: string, _oldValue: string | null, value: string | null): void {
      lifecycle.push(`status:${value}`);
      const output = this.shadowRoot?.querySelector("#upgrade-status");
      if (output) {
        output.textContent = value ?? "missing";
      }
    }
  }
  customElements.define(elementName, DetachedUpgradeElement);
  const state = document.createElement("p");
  state.dataset.upgradeState = "undefined-detached";
  state.textContent = "undefined while detached";
  host.append(state);
  assertFixture(!(element instanceof DetachedUpgradeElement), "definition does not auto-upgrade detached tree");
  await captureBoundary(host, capture, "boundary-1", "undefined-detached");

  customElements.upgrade(container);
  assertFixture(element instanceof DetachedUpgradeElement, "explicit upgrade upgrades detached tree");
  state.dataset.upgradeState = "upgraded-connected";
  state.textContent = "upgraded and connected";
  host.append(container);
  await captureBoundary(host, capture, "boundary-2", "upgraded-connected");
  element.setAttribute("status", "ready");
  return [
    fact("explicit-upgrade", element instanceof DetachedUpgradeElement),
    fact("connected", element.isConnected),
    fact("lifecycle", lifecycle.join("|")),
    fact("shadow-status", element.shadowRoot?.querySelector("output")?.textContent ?? "missing"),
  ];
}

async function customElementReconnectShadowSlot(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CaptureBoundary,
): Promise<BoundaryFact[]> {
  const lifecycle: string[] = [];
  const elementName = `x-reconnect-${meta.framework}-${spec.seed}`;
  class ReconnectElement extends HTMLElement {
    static get observedAttributes(): string[] {
      return ["status"];
    }

    constructor() {
      super();
      const root = this.attachShadow({ mode: "open" });
      root.innerHTML =
        '<article><slot name="label">label fallback</slot><slot>body fallback</slot><output>boot</output></article>';
    }

    connectedCallback(): void {
      lifecycle.push("connected");
    }

    disconnectedCallback(): void {
      lifecycle.push("disconnected");
    }

    attributeChangedCallback(_name: string, _oldValue: string | null, value: string | null): void {
      lifecycle.push(`status:${value}`);
      const output = this.shadowRoot?.querySelector("output");
      if (output) {
        output.textContent = value ?? "missing";
      }
    }
  }
  customElements.define(elementName, ReconnectElement);
  const element = document.createElement(elementName);
  element.id = "reconnect-element";
  const label = document.createElement("strong");
  label.slot = "label";
  label.textContent = "primary label";
  const detail = document.createElement("em");
  detail.textContent = "secondary detail";
  element.append(label, detail);
  host.append(element);
  await captureBoundary(host, capture, "boundary-1", "initial-slots");

  element.remove();
  label.removeAttribute("slot");
  detail.slot = "label";
  element.prepend(detail);
  host.append(element);
  await captureBoundary(host, capture, "boundary-2", "reassigned-slots");
  element.setAttribute("status", "ready");
  const namedSlot = element.shadowRoot?.querySelector('slot[name="label"]');
  const defaultSlot = element.shadowRoot?.querySelector("slot:not([name])");
  assertFixture(namedSlot instanceof HTMLSlotElement, "named reconnect slot exists");
  assertFixture(defaultSlot instanceof HTMLSlotElement, "default reconnect slot exists");
  return [
    fact("named-slot", namedSlot.assignedNodes().map((node) => node.textContent).join("|")),
    fact("default-slot", defaultSlot.assignedNodes().map((node) => node.textContent).join("|")),
    fact("lifecycle", lifecycle.join("|")),
    fact("shadow-status", element.shadowRoot?.querySelector("output")?.textContent ?? "missing"),
  ];
}

async function iframeDetachRecreateContext(
  host: HTMLElement,
  meta: SmokeMeta,
  capture: CaptureBoundary,
): Promise<BoundaryFact[]> {
  const first = await appendBoundaryFrame(host, {
    title: "first detachable context",
    scenario: "detach-context",
    phase: "first",
    src: "/support/boundary-frame.html?scenario=detach-context&phase=first",
  });
  const oldDocument = first.frame.contentDocument;
  const oldWindow = first.frame.contentWindow;
  assertFixture(oldDocument && oldWindow, "first frame exposes its context");
  await captureBoundary(host, capture, "boundary-1", "first-context");

  first.frame.remove();
  const second = await appendBoundaryFrame(host, {
    title: "replacement alternate context",
    scenario: "detach-context",
    phase: "second",
    src: "/support/alternate-origin-frame?scenario=detach-context&phase=second",
  });
  assertFixture(second.frame.contentWindow !== oldWindow, "replacement frame has a new WindowProxy");
  await captureBoundary(host, capture, "boundary-2", "replacement-context");
  const rendered = await sendBoundaryCommand(second.frame, "render", "rendered", {
    label: `${meta.framework}-replacement-ready`,
  });
  return [
    fact("old-frame-connected", first.frame.isConnected),
    fact("old-document-retained", oldDocument !== null),
    fact("new-window-proxy", second.frame.contentWindow !== oldWindow),
    fact("replacement-render", rendered.label),
  ];
}

export async function runBrowsingContextBoundaryCase(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CaptureBoundary,
): Promise<BrowsingContextBoundaryResult> {
  let facts: BoundaryFact[];
  switch (spec.slug) {
    case "cross-origin-messagechannel":
      facts = await crossOriginMessageChannel(host, meta, capture);
      break;
    case "cross-origin-frame-renavigation":
      facts = await crossOriginFrameRenavigation(host, meta, capture);
      break;
    case "same-to-cross-origin-transition":
      facts = await sameToCrossOriginTransition(host, meta, capture);
      break;
    case "nested-cross-origin-frame-chain":
      facts = await nestedCrossOriginFrameChain(host, capture);
      break;
    case "sandboxed-opaque-origin-frame":
      facts = await sandboxedOpaqueOriginFrame(host, capture);
      break;
    case "iframe-srcdoc-realm-replacement":
      facts = await iframeSrcdocRealmReplacement(host, meta, spec, capture);
      break;
    case "custom-element-adopt-across-documents":
      facts = await customElementAdoptAcrossDocuments(host, meta, spec, capture);
      break;
    case "detached-custom-element-upgrade":
      facts = await detachedCustomElementUpgrade(host, meta, spec, capture);
      break;
    case "custom-element-reconnect-shadow-slot":
      facts = await customElementReconnectShadowSlot(host, meta, spec, capture);
      break;
    case "iframe-detach-recreate-context":
      facts = await iframeDetachRecreateContext(host, meta, capture);
      break;
    default:
      throw new Error(`unknown browsing-context boundary case: ${spec.slug}`);
  }
  return { status: "ready", facts };
}

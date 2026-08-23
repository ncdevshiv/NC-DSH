import { assertFixture, microtaskTurns } from "./harness";
import {
  capturePlatformStep,
  fact,
  type CapturePlatformFrame,
  type PlatformBoundaryResult,
  type PlatformFact,
} from "./platform-boundary";
import type { CaseSpec, SmokeMeta } from "./types";

type ObserverScenario = (
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
) => Promise<PlatformFact[]>;

function scenarioRoot(host: HTMLElement, name: string): HTMLElement {
  const root = document.createElement("section");
  root.dataset.observerScenario = name;
  host.append(root);
  return root;
}

function output(root: HTMLElement, name: string, value: string): HTMLOutputElement {
  const element = document.createElement("output");
  element.dataset.observerOutput = name;
  element.textContent = value;
  root.append(element);
  return element;
}

function nodeLabel(node: Node): string {
  if (node instanceof Element) {
    return node.id ? `${node.localName}#${node.id}` : node.localName;
  }
  if (node instanceof Text) {
    return `#text(${node.data})`;
  }
  return node.nodeName;
}

function recordLabel(record: MutationRecord): string {
  if (record.type === "attributes") {
    return `attributes:${nodeLabel(record.target)}:${record.attributeName}:${record.oldValue}`;
  }
  if (record.type === "characterData") {
    return `characterData:${nodeLabel(record.target)}:${record.oldValue}`;
  }
  return `childList:${nodeLabel(record.target)}:+${Array.from(record.addedNodes, nodeLabel).join(",")}:-${Array.from(record.removedNodes, nodeLabel).join(",")}`;
}

async function flushObserverDelivery(): Promise<void> {
  await microtaskTurns(3);
}

async function mutationSubtreeOldValues(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const tree = document.createElement("div");
  tree.dataset.observedTree = "";
  const paragraph = document.createElement("p");
  paragraph.id = "subject";
  paragraph.dataset.state = "initial";
  const text = document.createTextNode(`alpha-${spec.seed}`);
  paragraph.append(text);
  tree.append(paragraph);
  root.append(tree);

  const labels: string[] = [];
  const observer = new MutationObserver((records) => labels.push(...records.map(recordLabel)));
  observer.observe(tree, {
    subtree: true,
    childList: true,
    attributes: true,
    attributeOldValue: true,
    characterData: true,
    characterDataOldValue: true,
  });

  paragraph.dataset.state = "updated";
  text.data = `beta-${spec.variant}`;
  const emphasis = document.createElement("em");
  emphasis.textContent = "added";
  paragraph.append(emphasis);
  await flushObserverDelivery();
  assertFixture(labels.length === 3, "observer delivered attribute, text, and child records");
  output(root, "first-batch", labels.join("\n"));
  await capturePlatformStep(host, capture, "platform-1", "observer-first-batch", [
    labels.length,
    paragraph.dataset.state,
    paragraph.textContent,
  ]);

  paragraph.removeAttribute("data-state");
  emphasis.replaceWith(document.createTextNode("replacement"));
  await flushObserverDelivery();
  assertFixture(Number(labels.length) === 5, "observer delivered the second mutation batch");
  output(root, "second-batch", labels.slice(3).join("\n"));
  await capturePlatformStep(host, capture, "platform-2", "observer-second-batch", [
    labels.length,
    paragraph.hasAttribute("data-state"),
    paragraph.childNodes.length,
  ]);
  observer.disconnect();

  return [
    fact("record-count", labels.length),
    fact("record-types", labels.map((label) => label.split(":", 1)[0]).join("|")),
    fact("first-old-values", `${labels[0]}|${labels[1]}`),
    fact("final-text", paragraph.textContent ?? ""),
  ];
}

async function takeRecordsDisconnect(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const target = document.createElement("div");
  target.dataset.target = "";
  root.append(target);
  const callbackBatches: string[][] = [];
  const observer = new MutationObserver((records) => {
    callbackBatches.push(records.map(recordLabel));
  });
  const options: MutationObserverInit = {
    attributes: true,
    attributeOldValue: true,
    childList: true,
  };
  observer.observe(target, options);

  target.append(document.createElement("span"));
  target.setAttribute("data-phase", "queued");
  const drained = observer.takeRecords().map(recordLabel);
  await flushObserverDelivery();
  assertFixture(drained.length === 2, "takeRecords synchronously drained both records");
  assertFixture(callbackBatches.length === 0, "drained records were not delivered to callback");
  observer.disconnect();
  target.setAttribute("data-phase", "disconnected");
  target.append(document.createElement("i"));
  await flushObserverDelivery();
  output(root, "drained", drained.join("\n"));
  await capturePlatformStep(host, capture, "platform-1", "observer-drain-disconnect", [
    drained.length,
    callbackBatches.length,
    target.children.length,
  ]);

  observer.observe(target, options);
  target.setAttribute("data-phase", `reconnected-${spec.variant}`);
  const strong = document.createElement("strong");
  strong.textContent = "live";
  target.append(strong);
  await flushObserverDelivery();
  assertFixture(Number(callbackBatches.length) === 1, "reconnected observer delivered one batch");
  output(root, "callback", callbackBatches.flat().join("\n"));
  await capturePlatformStep(host, capture, "platform-2", "observer-reconnect", [
    callbackBatches.length,
    callbackBatches[0]?.length ?? 0,
    target.dataset.phase,
  ]);
  observer.disconnect();

  return [
    fact("drained", drained.join("|")),
    fact("callback-batches", callbackBatches.length),
    fact("callback-records", callbackBatches.flat().join("|")),
    fact("children", Array.from(target.children, (item) => item.localName).join("|")),
  ];
}

async function fragmentReparentRecords(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const tree = document.createElement("div");
  const left = document.createElement("section");
  const right = document.createElement("section");
  left.id = "left";
  right.id = "right";
  tree.append(left, right);
  root.append(tree);
  const batches: string[][] = [];
  const observer = new MutationObserver((records) => batches.push(records.map(recordLabel)));
  observer.observe(tree, { childList: true, subtree: true });

  const fragment = document.createDocumentFragment();
  const first = document.createElement("b");
  const second = document.createElement("i");
  first.id = `first-${spec.seed}`;
  second.id = `second-${spec.seed}`;
  fragment.append(first, second);
  left.append(fragment);
  await flushObserverDelivery();
  assertFixture(batches.length === 1, "fragment insertion delivered one callback batch");
  assertFixture(batches[0]?.length === 1, "fragment insertion produced one childList record");
  output(root, "fragment", batches[0]?.join("\n") ?? "missing");
  await capturePlatformStep(host, capture, "platform-1", "observer-fragment", [
    left.children.length,
    fragment.childNodes.length,
    batches[0]?.length ?? 0,
  ]);

  right.append(first);
  right.prepend(second);
  await flushObserverDelivery();
  assertFixture(Number(batches.length) === 2, "two reparents shared the next callback batch");
  assertFixture(batches[1]?.length === 4, "two reparents produced remove/add record pairs");
  output(root, "reparent", batches[1]?.join("\n") ?? "missing");
  await capturePlatformStep(host, capture, "platform-2", "observer-reparent", [
    left.children.length,
    Array.from(right.children, (item) => item.id).join(","),
    batches[1]?.length ?? 0,
  ]);
  observer.disconnect();

  return [
    fact("batch-sizes", batches.map((batch) => batch.length).join("|")),
    fact("fragment-record", batches[0]?.[0] ?? "missing"),
    fact("reparent-records", batches[1]?.join("|") ?? "missing"),
    fact("right-order", Array.from(right.children, (item) => item.id).join("|")),
  ];
}

async function observerReentrantMicrotasks(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const target = document.createElement("div");
  target.dataset.reentrantTarget = "";
  root.append(target);
  const timeline: string[] = [];
  let callbackCount = 0;
  const observer = new MutationObserver((records) => {
    callbackCount += 1;
    timeline.push(`observer-${callbackCount}:${records.map((record) => record.attributeName).join(",")}`);
    if (callbackCount === 1) {
      target.setAttribute("data-reentrant", `r-${spec.variant}`);
      queueMicrotask(() => timeline.push("callback-microtask"));
    }
  });
  observer.observe(target, { attributes: true });

  target.setAttribute("data-first", "one");
  Promise.resolve().then(() => timeline.push("outer-promise"));
  await flushObserverDelivery();
  assertFixture(callbackCount === 2, "reentrant mutation produced a second observer delivery");
  output(root, "reentrant-first", timeline.join("\n"));
  await capturePlatformStep(host, capture, "platform-1", "observer-reentrant", [
    callbackCount,
    timeline.join(","),
  ]);

  timeline.push("before-event");
  target.addEventListener(
    "observer-probe",
    () => {
      timeline.push("event-handler");
      target.setAttribute("data-event", String(spec.seed));
      queueMicrotask(() => timeline.push("event-microtask"));
    },
    { once: true },
  );
  target.dispatchEvent(new Event("observer-probe"));
  timeline.push("after-event");
  await flushObserverDelivery();
  assertFixture(Number(callbackCount) === 3, "event mutation reached the observer");
  output(root, "reentrant-event", timeline.join("\n"));
  await capturePlatformStep(host, capture, "platform-2", "observer-event", [
    callbackCount,
    timeline.slice(-5).join(","),
  ]);
  observer.disconnect();

  return [
    fact("callback-count", callbackCount),
    fact("timeline", timeline.join("|")),
    fact("reentrant-value", target.dataset.reentrant ?? "missing"),
    fact("event-value", target.dataset.event ?? "missing"),
  ];
}

async function customElementUpgradeReactions(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const tag = `x-upgrade-${spec.seed}`;
  const reactions: string[] = [];
  const candidate = document.createElement(tag);
  candidate.setAttribute("data-state", "before");
  candidate.textContent = "candidate";
  root.append(candidate);

  class UpgradeProbe extends HTMLElement {
    static observedAttributes = ["data-state"];

    constructor() {
      super();
      reactions.push("constructor");
    }

    connectedCallback(): void {
      reactions.push("connected");
    }

    disconnectedCallback(): void {
      reactions.push("disconnected");
    }

    attributeChangedCallback(name: string, oldValue: string | null, value: string | null): void {
      reactions.push(`attribute:${name}:${oldValue}:${value}`);
    }
  }

  customElements.define(tag, UpgradeProbe);
  await customElements.whenDefined(tag);
  await flushObserverDelivery();
  assertFixture(candidate instanceof UpgradeProbe, "definition upgraded the connected candidate");
  output(root, "upgrade", reactions.join("\n"));
  await capturePlatformStep(host, capture, "platform-1", "custom-upgrade", [
    candidate.constructor.name,
    reactions.length,
    reactions.join(","),
  ]);

  candidate.setAttribute("data-state", `after-${spec.variant}`);
  candidate.remove();
  root.prepend(candidate);
  await flushObserverDelivery();
  output(root, "reconnect", reactions.slice(3).join("\n"));
  await capturePlatformStep(host, capture, "platform-2", "custom-reconnect", [
    reactions.length,
    candidate.isConnected,
    candidate.getAttribute("data-state"),
  ]);

  return [
    fact("instance-upgraded", candidate instanceof UpgradeProbe),
    fact("reactions", reactions.join("|")),
    fact("state", candidate.getAttribute("data-state") ?? "missing"),
    fact("position", candidate === root.firstElementChild),
  ];
}

async function customElementAdoptReactions(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const tag = `x-adopt-${spec.seed}`;
  const reactions: string[] = [];

  class AdoptProbe extends HTMLElement {
    connectedCallback(): void {
      reactions.push("connected");
    }

    disconnectedCallback(): void {
      reactions.push("disconnected");
    }

    adoptedCallback(): void {
      reactions.push("adopted");
    }
  }

  customElements.define(tag, AdoptProbe);
  const element = document.createElement(tag);
  element.textContent = `adopt-${spec.variant}`;
  root.append(element);
  const other = document.implementation.createHTMLDocument(`Other ${spec.seed}`);
  other.adoptNode(element);
  other.body.append(element);
  await flushObserverDelivery();
  assertFixture(element.ownerDocument === other, "first adoption changed ownerDocument");
  output(root, "adopt-away", reactions.join("\n"));
  await capturePlatformStep(host, capture, "platform-1", "custom-adopt-away", [
    reactions.join(","),
    element.ownerDocument === other,
    element.isConnected,
  ]);

  document.adoptNode(element);
  root.append(element);
  await flushObserverDelivery();
  assertFixture(element.ownerDocument === document, "second adoption restored ownerDocument");
  output(root, "adopt-back", reactions.join("\n"));
  await capturePlatformStep(host, capture, "platform-2", "custom-adopt-back", [
    reactions.join(","),
    element.ownerDocument === document,
    element.isConnected,
  ]);

  return [
    fact("reactions", reactions.join("|")),
    fact("owner-restored", element.ownerDocument === document),
    fact("connected", element.isConnected),
    fact("text", element.textContent ?? ""),
  ];
}

async function shadowSlotchangeAssignment(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const shadowHost = document.createElement("div");
  shadowHost.dataset.shadowHost = "";
  const shadow = shadowHost.attachShadow({ mode: "open" });
  shadow.innerHTML = `<header><slot name="title"></slot></header><main><slot></slot></main>`;
  const namedSlot = shadow.querySelector('slot[name="title"]');
  const defaultSlot = shadow.querySelector("main slot");
  assertFixture(namedSlot instanceof HTMLSlotElement, "named slot exists");
  assertFixture(defaultSlot instanceof HTMLSlotElement, "default slot exists");
  const events: string[] = [];
  namedSlot.addEventListener("slotchange", () => events.push("title"));
  defaultSlot.addEventListener("slotchange", () => events.push("default"));
  const title = document.createElement("h2");
  title.slot = "title";
  title.textContent = `Title ${spec.seed}`;
  const body = document.createElement("p");
  body.textContent = "Body";
  shadowHost.append(title, body);
  root.append(shadowHost);
  await flushObserverDelivery();
  assertFixture(namedSlot.assignedElements()[0] === title, "title entered the named slot");
  assertFixture(defaultSlot.assignedElements()[0] === body, "body entered the default slot");
  output(root, "slot-first", events.join(","));
  await capturePlatformStep(host, capture, "platform-1", "slot-assignment", [
    namedSlot.assignedElements().map(nodeLabel).join(","),
    defaultSlot.assignedElements().map(nodeLabel).join(","),
    events.join(","),
  ]);

  body.slot = "title";
  const replacement = document.createElement("span");
  replacement.textContent = `replacement-${spec.variant}`;
  shadowHost.prepend(replacement);
  await flushObserverDelivery();
  output(root, "slot-second", events.join(","));
  await capturePlatformStep(host, capture, "platform-2", "slot-reassignment", [
    namedSlot.assignedElements().map(nodeLabel).join(","),
    defaultSlot.assignedElements().map(nodeLabel).join(","),
    events.join(","),
  ]);

  return [
    fact("events", events.join("|")),
    fact("named", namedSlot.assignedElements().map(nodeLabel).join("|")),
    fact("default", defaultSlot.assignedElements().map(nodeLabel).join("|")),
    fact("shadow-text", shadow.textContent?.replaceAll(/\s+/g, " ").trim() ?? ""),
  ];
}

async function shadowObserverBoundary(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const shadowHost = document.createElement("div");
  const light = document.createElement("span");
  light.textContent = "light";
  shadowHost.append(light);
  const shadow = shadowHost.attachShadow({ mode: "open" });
  const internal = document.createElement("strong");
  internal.textContent = "shadow";
  shadow.append(internal, document.createElement("slot"));
  root.append(shadowHost);
  const hostRecords: string[] = [];
  const shadowRecords: string[] = [];
  const hostObserver = new MutationObserver((records) => hostRecords.push(...records.map(recordLabel)));
  const shadowObserver = new MutationObserver((records) =>
    shadowRecords.push(...records.map(recordLabel)),
  );
  hostObserver.observe(shadowHost, { subtree: true, childList: true, attributes: true });
  shadowObserver.observe(shadow, { subtree: true, childList: true, attributes: true });

  internal.dataset.phase = "one";
  light.dataset.phase = "one";
  internal.append(document.createTextNode(`-${spec.variant}`));
  await flushObserverDelivery();
  assertFixture(hostRecords.length === 1, "host observer saw only the light DOM mutation");
  assertFixture(shadowRecords.length === 2, "shadow observer saw both internal mutations");
  output(root, "host-records", hostRecords.join("\n"));
  output(root, "shadow-records", shadowRecords.join("\n"));
  await capturePlatformStep(host, capture, "platform-1", "shadow-observer-scope", [
    hostRecords.length,
    shadowRecords.length,
    internal.textContent,
  ]);

  hostObserver.disconnect();
  light.append(document.createElement("i"));
  internal.setAttribute("data-phase", "two");
  shadow.append(document.createElement("em"));
  await flushObserverDelivery();
  assertFixture(hostRecords.length === 1, "disconnected host observer stayed silent");
  assertFixture(Number(shadowRecords.length) === 4, "shadow observer remained live");
  output(root, "shadow-second", shadowRecords.slice(2).join("\n"));
  await capturePlatformStep(host, capture, "platform-2", "shadow-observer-disconnect", [
    hostRecords.length,
    shadowRecords.length,
    shadow.childNodes.length,
  ]);
  shadowObserver.disconnect();

  return [
    fact("host-records", hostRecords.join("|")),
    fact("shadow-records", shadowRecords.join("|")),
    fact("light-children", light.childNodes.length),
    fact("shadow-children", shadow.childNodes.length),
  ];
}

async function eventMicrotaskMutationOrder(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const target = document.createElement("div");
  const button = document.createElement("button");
  button.textContent = "mutate";
  target.append(button);
  root.append(target);
  const timeline: string[] = [];
  let clicks = 0;
  const observer = new MutationObserver((records) => {
    timeline.push(`observer:${records.map((record) => record.type).join(",")}`);
  });
  observer.observe(target, { subtree: true, childList: true, attributes: true });
  button.addEventListener("click", () => {
    clicks += 1;
    timeline.push(`handler-${clicks}:start`);
    const item = document.createElement("span");
    item.textContent = `item-${clicks}`;
    target.append(item);
    queueMicrotask(() => timeline.push(`queue-${clicks}`));
    Promise.resolve().then(() => {
      target.dataset.promise = `${spec.seed}-${clicks}`;
      timeline.push(`promise-${clicks}`);
    });
    timeline.push(`handler-${clicks}:end`);
  });

  button.click();
  timeline.push("after-click-1");
  await flushObserverDelivery();
  output(root, "event-first", timeline.join("\n"));
  await capturePlatformStep(host, capture, "platform-1", "event-observer-order", [
    clicks,
    timeline.join(","),
    target.children.length,
  ]);

  button.dispatchEvent(new MouseEvent("click", { bubbles: true, composed: true }));
  timeline.push("after-click-2");
  await flushObserverDelivery();
  output(root, "event-second", timeline.join("\n"));
  await capturePlatformStep(host, capture, "platform-2", "event-observer-repeat", [
    clicks,
    timeline.slice(-7).join(","),
    target.dataset.promise,
  ]);
  observer.disconnect();

  return [
    fact("clicks", clicks),
    fact("timeline", timeline.join("|")),
    fact("items", target.querySelectorAll("span").length),
    fact("promise-state", target.dataset.promise ?? "missing"),
  ];
}

async function replaceNormalizeRecords(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const paragraph = document.createElement("p");
  const left = document.createTextNode("A");
  const middle = document.createElement("span");
  middle.textContent = "B";
  const right = document.createTextNode("C");
  paragraph.append(left, middle, right);
  root.append(paragraph);
  const batches: string[][] = [];
  const observer = new MutationObserver((records) => batches.push(records.map(recordLabel)));
  observer.observe(paragraph, {
    childList: true,
    subtree: true,
    characterData: true,
    characterDataOldValue: true,
  });

  const emphasis = document.createElement("em");
  emphasis.textContent = `E${spec.variant}`;
  middle.replaceWith(document.createTextNode("X"), emphasis);
  left.replaceWith("alpha", document.createTextNode("-"));
  await flushObserverDelivery();
  assertFixture(batches.length === 1, "replace operations shared one observer delivery");
  output(root, "replace", batches[0]?.join("\n") ?? "missing");
  await capturePlatformStep(host, capture, "platform-1", "replace-records", [
    paragraph.textContent,
    paragraph.childNodes.length,
    batches[0]?.length ?? 0,
  ]);

  paragraph.append(
    document.createTextNode("-"),
    document.createTextNode(`tail-${spec.seed}`),
    document.createTextNode(""),
  );
  paragraph.normalize();
  await flushObserverDelivery();
  assertFixture(paragraph.childNodes.length === 3, "normalize merged adjacent trailing text nodes");
  output(root, "normalize", batches.at(-1)?.join("\n") ?? "missing");
  await capturePlatformStep(host, capture, "platform-2", "normalize-records", [
    paragraph.textContent,
    paragraph.childNodes.length,
    batches.at(-1)?.length ?? 0,
  ]);
  observer.disconnect();

  return [
    fact("batch-sizes", batches.map((batch) => batch.length).join("|")),
    fact("records", batches.flat().join("|")),
    fact("child-kinds", Array.from(paragraph.childNodes, nodeLabel).join("|")),
    fact("text", paragraph.textContent ?? ""),
  ];
}

const SCENARIOS: Record<string, ObserverScenario> = {
  "mutation-subtree-old-values": mutationSubtreeOldValues,
  "take-records-disconnect": takeRecordsDisconnect,
  "fragment-reparent-records": fragmentReparentRecords,
  "observer-reentrant-microtasks": observerReentrantMicrotasks,
  "custom-element-upgrade-reactions": customElementUpgradeReactions,
  "custom-element-adopt-reactions": customElementAdoptReactions,
  "shadow-slotchange-assignment": shadowSlotchangeAssignment,
  "shadow-observer-boundary": shadowObserverBoundary,
  "event-microtask-mutation-order": eventMicrotaskMutationOrder,
  "replace-normalize-records": replaceNormalizeRecords,
};

export async function runDomObserverReactionCase(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformBoundaryResult> {
  const scenario = SCENARIOS[spec.slug];
  assertFixture(scenario, `missing DOM observer scenario ${spec.slug}`);
  const facts = await scenario(host, meta, spec, capture);
  return { status: "ready", facts };
}

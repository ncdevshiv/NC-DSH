import { assertFixture, microtaskTurns } from "./harness";
import {
  capturePlatformStep,
  fact,
  type CapturePlatformFrame,
  type PlatformBoundaryResult,
  type PlatformFact,
} from "./platform-boundary";
import type { CaseSpec, SmokeMeta } from "./types";

type EventScenario = (
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
) => Promise<PlatformFact[]>;

function scenarioRoot(host: HTMLElement, name: string): HTMLElement {
  const root = document.createElement("section");
  root.dataset.eventScenario = name;
  host.append(root);
  return root;
}

function output(root: HTMLElement, name: string, value: string): HTMLOutputElement {
  const element = document.createElement("output");
  element.dataset.eventOutput = name;
  element.textContent = value;
  root.append(element);
  return element;
}

function eventId(element: Element, value: string): void {
  element.setAttribute("data-event-id", value);
}

function label(target: EventTarget | null): string {
  if (target === null) {
    return "null";
  }
  if (target === window) {
    return "window";
  }
  if (target === document) {
    return "document";
  }
  if (target instanceof ShadowRoot) {
    return `shadow(${label(target.host)})`;
  }
  if (target instanceof Element) {
    return target.getAttribute("data-event-id") ?? target.localName;
  }
  return Object.prototype.toString.call(target);
}

function pathLabel(event: Event): string {
  return event.composedPath().map(label).join(">");
}

async function captureBubbleOrder(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  eventId(root, "root");
  const outer = document.createElement("div");
  eventId(outer, "outer");
  const button = document.createElement("button");
  button.type = "button";
  button.textContent = `${meta.framework}-${spec.variant}`;
  eventId(button, "button");
  outer.append(button);
  root.append(outer);

  const first: string[] = [];
  const second: string[] = [];
  let active = first;
  const observe = (owner: EventTarget, name: string, capturePhase: boolean): void => {
    owner.addEventListener(
      "click",
      (event) => {
        active.push(
          `${name}:${capturePhase ? "capture" : "bubble"}:${event.eventPhase}:${label(event.target)}:${label(event.currentTarget)}`,
        );
      },
      capturePhase,
    );
  };
  observe(document, "document", true);
  observe(root, "root", true);
  observe(outer, "outer", true);
  observe(button, "button", true);
  observe(button, "button", false);
  observe(outer, "outer", false);
  observe(root, "root", false);
  observe(document, "document", false);

  button.click();
  assertFixture(first.length === 8, "click visited each capture and bubble observer");
  assertFixture(first[0].startsWith("document:capture"), "document capture ran first");
  assertFixture(first.at(-1)?.startsWith("document:bubble"), "document bubble ran last");
  output(root, "click-first", first.join("\n"));
  await capturePlatformStep(host, capture, "platform-1", "click-first-dispatch", [
    first.length,
    first.join(","),
  ]);

  active = second;
  let onceCount = 0;
  button.addEventListener(
    "click",
    () => {
      onceCount += 1;
      second.push(`once:${onceCount}`);
    },
    { once: true },
  );
  button.click();
  button.click();
  assertFixture(onceCount === 1, "once listener ran exactly once across two clicks");
  assertFixture(second.filter((entry) => entry.startsWith("document:capture")).length === 2, "both later clicks propagated");
  output(root, "click-second", `${second.join("\n")}\nonce=${onceCount}`);
  await capturePlatformStep(host, capture, "platform-2", "click-repeated", [
    second.length,
    onceCount,
    second.join(","),
  ]);

  return [
    fact("first-order", first.join("|")),
    fact("repeat-order", second.join("|")),
    fact("once", onceCount),
    fact("target", button.textContent),
  ];
}

async function propagationStopBoundaries(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  eventId(root, "root");
  const middle = document.createElement("div");
  eventId(middle, "middle");
  const leaf = document.createElement("span");
  eventId(leaf, "leaf");
  middle.append(leaf);
  root.append(middle);

  const soft: string[] = [];
  root.addEventListener("soft-stop", () => soft.push("root-capture"), true);
  middle.addEventListener("soft-stop", (event) => {
    soft.push("middle-stop");
    event.stopPropagation();
  });
  middle.addEventListener("soft-stop", () => soft.push("middle-later"));
  root.addEventListener("soft-stop", () => soft.push("root-bubble"));
  leaf.dispatchEvent(new CustomEvent("soft-stop", { bubbles: true, composed: true }));
  assertFixture(soft.join("|") === "root-capture|middle-stop|middle-later", "stopPropagation kept same-target listeners only");
  output(root, "soft-stop", soft.join("|"));
  await capturePlatformStep(host, capture, "platform-1", "stop-propagation", [
    soft.join(","),
    soft.length,
  ]);

  const hard: string[] = [];
  root.addEventListener("hard-stop", () => hard.push("root-capture"), true);
  middle.addEventListener("hard-stop", (event) => {
    hard.push("middle-immediate");
    event.stopImmediatePropagation();
  });
  middle.addEventListener("hard-stop", () => hard.push("middle-skipped"));
  root.addEventListener("hard-stop", () => hard.push("root-skipped"));
  leaf.dispatchEvent(new CustomEvent("hard-stop", { bubbles: true, composed: true }));
  assertFixture(hard.join("|") === "root-capture|middle-immediate", "stopImmediatePropagation skipped later listeners");
  output(root, "hard-stop", hard.join("|"));
  await capturePlatformStep(host, capture, "platform-2", "stop-immediate", [
    hard.join(","),
    hard.length,
  ]);

  return [
    fact("soft", soft.join("|")),
    fact("hard", hard.join("|")),
    fact("soft-count", soft.length),
    fact("hard-count", hard.length),
  ];
}

async function listenerOptionsSignal(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const target = document.createElement("button");
  target.type = "button";
  target.textContent = meta.framework;
  eventId(target, "signal-target");
  root.append(target);
  const controller = new AbortController();
  const events: string[] = [];
  let onceCount = 0;
  let signalCount = 0;
  target.addEventListener(
    "fixture-wheel",
    (event) => {
      event.preventDefault();
      events.push(`passive:${event.defaultPrevented}`);
    },
    { passive: true },
  );
  target.addEventListener(
    "fixture-wheel",
    () => {
      onceCount += 1;
      events.push(`once:${onceCount}`);
    },
    { once: true },
  );
  target.addEventListener(
    "fixture-wheel",
    () => {
      signalCount += 1;
      events.push(`signal:${signalCount}`);
    },
    { signal: controller.signal },
  );
  controller.signal.addEventListener("abort", () => events.push(`abort:${String(controller.signal.reason)}`));

  const first = new CustomEvent("fixture-wheel", { cancelable: true });
  const firstResult = target.dispatchEvent(first);
  assertFixture(firstResult && !first.defaultPrevented, "passive listener could not cancel dispatch");
  output(root, "options-first", `${events.join("|")}\n${firstResult}:${first.defaultPrevented}`);
  await capturePlatformStep(host, capture, "platform-1", "listener-options-first", [
    events.join(","),
    firstResult,
    first.defaultPrevented,
  ]);

  controller.abort(`stop-${meta.framework}-${spec.variant}`);
  target.addEventListener("fixture-wheel", (event) => {
    events.push("active-cancel");
    event.preventDefault();
  });
  const second = new CustomEvent("fixture-wheel", { cancelable: true });
  const secondResult = target.dispatchEvent(second);
  assertFixture(!secondResult && second.defaultPrevented, "active listener canceled dispatch");
  assertFixture(onceCount === 1, "once listener was absent from second dispatch");
  assertFixture(signalCount === 1, "aborted listener was absent from second dispatch");
  output(root, "options-second", `${events.join("|")}\n${secondResult}:${second.defaultPrevented}`);
  await capturePlatformStep(host, capture, "platform-2", "listener-options-second", [
    events.join(","),
    secondResult,
    second.defaultPrevented,
    onceCount,
    signalCount,
  ]);

  return [
    fact("events", events.join("|")),
    fact("first-result", firstResult),
    fact("second-result", secondResult),
    fact("once", onceCount),
    fact("signal", signalCount),
  ];
}

async function shadowComposedRetarget(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const shadowHost = document.createElement("article");
  eventId(shadowHost, "shadow-host");
  root.append(shadowHost);
  const shadow = shadowHost.attachShadow({ mode: "open" });
  const button = document.createElement("button");
  button.type = "button";
  button.textContent = `${meta.framework}-${spec.seed}`;
  eventId(button, "shadow-button");
  shadow.append(button);
  const active: string[] = [];
  const listen = (target: EventTarget, owner: string): void => {
    target.addEventListener("shadow-boundary", (event) => {
      active.push(`${owner}:${label(event.target)}:${label(event.currentTarget)}:${pathLabel(event)}`);
    });
  };
  listen(shadow, "shadow");
  listen(shadowHost, "host");
  listen(root, "root");
  listen(document, "document");

  button.dispatchEvent(new CustomEvent("shadow-boundary", { bubbles: true, composed: false }));
  const confined = active.splice(0);
  assertFixture(confined.length === 1 && confined[0].startsWith("shadow:shadow-button"), "non-composed event stayed in shadow root");
  output(root, "shadow-confined", confined.join("\n"));
  await capturePlatformStep(host, capture, "platform-1", "shadow-non-composed", [
    confined.length,
    confined.join(","),
  ]);

  button.dispatchEvent(new CustomEvent("shadow-boundary", { bubbles: true, composed: true }));
  const escaped = active.splice(0);
  assertFixture(escaped.length === 4, "composed event reached host, root, and document");
  assertFixture(escaped[0].startsWith("shadow:shadow-button"), "inside listener retained internal target");
  assertFixture(escaped[1].startsWith("host:shadow-host"), "host listener observed retargeted host");
  output(root, "shadow-escaped", escaped.join("\n"));
  await capturePlatformStep(host, capture, "platform-2", "shadow-composed", [
    escaped.length,
    escaped.join(","),
  ]);

  return [
    fact("confined", confined.join("|")),
    fact("escaped", escaped.join("|")),
    fact("open-root", shadowHost.shadowRoot === shadow),
    fact("button", button.textContent),
  ];
}

async function closedShadowPath(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const shadowHost = document.createElement("div");
  eventId(shadowHost, "closed-host");
  root.append(shadowHost);
  const shadow = shadowHost.attachShadow({ mode: "closed" });
  const first = document.createElement("button");
  first.type = "button";
  first.textContent = `first-${spec.variant}`;
  eventId(first, "closed-first");
  shadow.append(first);
  const inside: string[] = [];
  const outside: string[] = [];
  shadow.addEventListener("closed-path", (event) => inside.push(`${label(event.target)}:${pathLabel(event)}`));
  document.addEventListener("closed-path", (event) => outside.push(`${label(event.target)}:${pathLabel(event)}`));

  first.dispatchEvent(new CustomEvent("closed-path", { bubbles: true, composed: true }));
  assertFixture(shadowHost.shadowRoot === null, "closed shadow root stayed hidden from host API");
  assertFixture(inside[0].startsWith("closed-first:"), "inside path exposed closed-root internals");
  assertFixture(!outside[0].includes("closed-first") && !outside[0].includes("shadow("), "outside path hid closed-root internals");
  output(root, "closed-first", `${inside.join("|")}\n${outside.join("|")}\napi=${shadowHost.shadowRoot}`);
  await capturePlatformStep(host, capture, "platform-1", "closed-shadow-first", [
    inside.join(","),
    outside.join(","),
    shadowHost.shadowRoot,
  ]);

  const second = document.createElement("button");
  second.type = "button";
  second.textContent = `second-${spec.seed}`;
  eventId(second, "closed-second");
  first.replaceWith(second);
  second.dispatchEvent(new CustomEvent("closed-path", { bubbles: true, composed: true }));
  assertFixture(inside.length === 2 && outside.length === 2, "replacement event crossed closed root");
  output(root, "closed-second", `${inside.join("|")}\n${outside.join("|")}\nchildren=${shadow.childNodes.length}`);
  await capturePlatformStep(host, capture, "platform-2", "closed-shadow-replaced", [
    inside.join(","),
    outside.join(","),
    shadow.childNodes.length,
  ]);

  return [
    fact("inside", inside.join("|")),
    fact("outside", outside.join("|")),
    fact("closed-api", shadowHost.shadowRoot),
    fact("children", shadow.childNodes.length),
  ];
}

async function slotReassignmentPath(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const shadowHost = document.createElement("div");
  eventId(shadowHost, "slot-host");
  const slotted = document.createElement("button");
  slotted.type = "button";
  slotted.slot = "primary";
  slotted.textContent = `${meta.framework}-${spec.variant}`;
  eventId(slotted, "slotted-button");
  shadowHost.append(slotted);
  root.append(shadowHost);
  const shadow = shadowHost.attachShadow({ mode: "open" });
  const primary = document.createElement("slot");
  primary.name = "primary";
  eventId(primary, "primary-slot");
  const fallback = document.createElement("slot");
  eventId(fallback, "default-slot");
  shadow.append(primary, fallback);
  const changes: string[] = [];
  primary.addEventListener("slotchange", () => changes.push(`primary:${primary.assignedElements().length}`));
  fallback.addEventListener("slotchange", () => changes.push(`default:${fallback.assignedElements().length}`));
  await microtaskTurns();

  let path = "";
  shadow.addEventListener("click", (event) => {
    path = pathLabel(event);
  });
  slotted.click();
  assertFixture(slotted.assignedSlot === primary, "named child used primary slot");
  assertFixture(path.includes("primary-slot"), "event path crossed named slot");
  output(root, "slot-primary", `${path}\n${changes.join("|")}\nassigned=${primary.assignedElements().map(label).join("|")}`);
  await capturePlatformStep(host, capture, "platform-1", "slot-primary", [
    path,
    changes.join(","),
    primary.assignedElements().map(label).join(","),
  ]);

  slotted.removeAttribute("slot");
  await microtaskTurns();
  path = "";
  slotted.click();
  assertFixture(slotted.assignedSlot === fallback, "unnamed child moved to default slot");
  assertFixture(path.includes("default-slot"), "event path crossed default slot");
  output(root, "slot-default", `${path}\n${changes.join("|")}\nassigned=${fallback.assignedElements().map(label).join("|")}`);
  await capturePlatformStep(host, capture, "platform-2", "slot-default", [
    path,
    changes.join(","),
    fallback.assignedElements().map(label).join(","),
  ]);

  return [
    fact("path", path),
    fact("changes", changes.join("|")),
    fact("primary-count", primary.assignedElements().length),
    fact("default-count", fallback.assignedElements().length),
  ];
}

async function focusBlurOrder(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const first = document.createElement("input");
  first.value = meta.framework;
  eventId(first, "focus-first");
  const second = document.createElement("input");
  second.value = String(spec.seed);
  eventId(second, "focus-second");
  root.append(first, second);
  const events: string[] = [];
  for (const type of ["focus", "blur", "focusin", "focusout"] as const) {
    root.addEventListener(
      type,
      (event) => {
        const focus = event as FocusEvent;
        events.push(`${type}:${label(focus.target)}:${label(focus.relatedTarget)}:${focus.bubbles}`);
      },
      type === "focus" || type === "blur",
    );
  }

  first.focus();
  assertFixture(document.activeElement === first, "first input became active");
  output(root, "focus-first", `${events.join("|")}\nactive=${label(document.activeElement)}`);
  await capturePlatformStep(host, capture, "platform-1", "focus-first", [
    events.join(","),
    label(document.activeElement),
  ]);

  second.focus();
  assertFixture(document.activeElement === second, "second input became active");
  assertFixture(events.some((entry) => entry.startsWith("blur:focus-first:focus-second")), "blur relatedTarget identified second input");
  assertFixture(events.some((entry) => entry.startsWith("focus:focus-second:focus-first")), "focus relatedTarget identified first input");
  output(root, "focus-second", `${events.join("|")}\nactive=${label(document.activeElement)}`);
  await capturePlatformStep(host, capture, "platform-2", "focus-second", [
    events.join(","),
    label(document.activeElement),
  ]);

  return [
    fact("events", events.join("|")),
    fact("active", label(document.activeElement)),
    fact("first-value", first.value),
    fact("second-value", second.value),
  ];
}

async function shadowFocusRetarget(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const shadowHost = document.createElement("div");
  shadowHost.tabIndex = -1;
  eventId(shadowHost, "focus-host");
  const outside = document.createElement("button");
  outside.type = "button";
  outside.textContent = `outside-${spec.variant}`;
  eventId(outside, "focus-outside");
  root.append(shadowHost, outside);
  const shadow = shadowHost.attachShadow({ mode: "open", delegatesFocus: false });
  const inside = document.createElement("input");
  inside.value = meta.framework;
  eventId(inside, "focus-inside");
  shadow.append(inside);
  const insideEvents: string[] = [];
  const outsideEvents: string[] = [];
  shadow.addEventListener("focusin", (event) => {
    const focus = event as FocusEvent;
    insideEvents.push(`in:${label(focus.target)}:${label(focus.relatedTarget)}:${pathLabel(focus)}`);
  });
  shadow.addEventListener("focusout", (event) => {
    const focus = event as FocusEvent;
    insideEvents.push(`out:${label(focus.target)}:${label(focus.relatedTarget)}:${pathLabel(focus)}`);
  });
  document.addEventListener("focusin", (event) => {
    const focus = event as FocusEvent;
    outsideEvents.push(`in:${label(focus.target)}:${label(focus.relatedTarget)}:${pathLabel(focus)}`);
  });
  document.addEventListener("focusout", (event) => {
    const focus = event as FocusEvent;
    outsideEvents.push(`out:${label(focus.target)}:${label(focus.relatedTarget)}:${pathLabel(focus)}`);
  });

  inside.focus();
  assertFixture(document.activeElement === shadowHost, "document retargeted activeElement to shadow host");
  assertFixture(shadow.activeElement === inside, "shadow root exposed internal activeElement");
  output(
    root,
    "shadow-focus-inside",
    `${insideEvents.join("|")}\n${outsideEvents.join("|")}\ndoc=${label(document.activeElement)}:shadow=${label(shadow.activeElement)}`,
  );
  await capturePlatformStep(host, capture, "platform-1", "shadow-focus-inside", [
    insideEvents.join(","),
    outsideEvents.join(","),
    label(document.activeElement),
    label(shadow.activeElement),
  ]);

  outside.focus();
  assertFixture(document.activeElement === outside, "focus moved outside the shadow root");
  assertFixture(shadow.activeElement === null, "shadow root cleared internal activeElement");
  output(
    root,
    "shadow-focus-outside",
    `${insideEvents.join("|")}\n${outsideEvents.join("|")}\ndoc=${label(document.activeElement)}:shadow=${label(shadow.activeElement)}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "shadow-focus-outside", [
    insideEvents.join(","),
    outsideEvents.join(","),
    label(document.activeElement),
    label(shadow.activeElement),
  ]);

  return [
    fact("inside-events", insideEvents.join("|")),
    fact("outside-events", outsideEvents.join("|")),
    fact("active", label(document.activeElement)),
    fact("shadow-active", label(shadow.activeElement)),
  ];
}

async function listenerMutationDispatch(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const target = document.createElement("div");
  eventId(target, "mutation-target");
  root.append(target);
  const first: string[] = [];
  const second: string[] = [];
  let active = first;
  const listenerB = (): void => {
    active.push("b");
  };
  const listenerD = (): void => {
    active.push("d");
  };
  const listenerA = (): void => {
    active.push("a");
    target.removeEventListener("listener-mutation", listenerB);
    target.addEventListener("listener-mutation", listenerD);
  };
  const listenerC = (): void => {
    active.push(`c-${spec.variant}`);
  };
  const objectListener = {
    handleEvent: (): void => {
      active.push("object");
    },
  };
  target.addEventListener("listener-mutation", listenerA);
  target.addEventListener("listener-mutation", listenerB);
  target.addEventListener("listener-mutation", listenerC);
  target.addEventListener("listener-mutation", objectListener);

  target.dispatchEvent(new Event("listener-mutation"));
  assertFixture(first.join("|") === `a|c-${spec.variant}|object`, "removed listener skipped and added listener waited");
  output(root, "mutation-first", first.join("|"));
  await capturePlatformStep(host, capture, "platform-1", "listener-mutation-first", [
    first.join(","),
    first.length,
  ]);

  active = second;
  target.dispatchEvent(new Event("listener-mutation"));
  assertFixture(second.join("|") === `a|c-${spec.variant}|object|d`, "added listener ran in the next dispatch");
  output(root, "mutation-second", second.join("|"));
  await capturePlatformStep(host, capture, "platform-2", "listener-mutation-second", [
    second.join(","),
    second.length,
  ]);

  return [
    fact("first", first.join("|")),
    fact("second", second.join("|")),
    fact("first-count", first.length),
    fact("second-count", second.length),
  ];
}

async function customEventRedispatch(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const target = document.createElement("button");
  target.type = "button";
  target.textContent = meta.framework;
  eventId(target, "redispatch-target");
  root.append(target);
  const detail = { owner: meta.framework, count: 0, values: [spec.seed, spec.variant] };
  const event = new CustomEvent("redispatch", {
    bubbles: true,
    cancelable: true,
    composed: true,
    detail,
  });
  const observations: string[] = [];
  let dispatchCount = 0;
  target.addEventListener("redispatch", (current) => {
    dispatchCount += 1;
    detail.count += 1;
    let nested = "missing";
    try {
      target.dispatchEvent(current);
    } catch (error: unknown) {
      nested = error instanceof DOMException ? error.name : Object.prototype.toString.call(error);
    }
    current.preventDefault();
    observations.push(
      `${dispatchCount}:${nested}:${current.eventPhase}:${current.defaultPrevented}:${pathLabel(current)}`,
    );
  });

  const firstResult = target.dispatchEvent(event);
  assertFixture(!firstResult && event.defaultPrevented, "first dispatch was canceled");
  assertFixture(observations[0].includes("InvalidStateError"), "nested redispatch threw InvalidStateError");
  assertFixture(event.composedPath().length === 0, "composedPath cleared after dispatch");
  output(
    root,
    "redispatch-first",
    `${observations.join("|")}\n${firstResult}:${event.defaultPrevented}:${event.eventPhase}\n${JSON.stringify(detail)}`,
  );
  await capturePlatformStep(host, capture, "platform-1", "custom-event-first", [
    observations.join(","),
    firstResult,
    event.defaultPrevented,
    detail.count,
  ]);

  const secondResult = target.dispatchEvent(event);
  assertFixture(!secondResult && dispatchCount === 2, "completed CustomEvent could be dispatched again");
  assertFixture(event.detail === detail && detail.count === 2, "redispatch retained detail identity");
  output(
    root,
    "redispatch-second",
    `${observations.join("|")}\n${secondResult}:${event.defaultPrevented}:${event.eventPhase}\n${JSON.stringify(detail)}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "custom-event-second", [
    observations.join(","),
    secondResult,
    event.defaultPrevented,
    detail.count,
  ]);

  return [
    fact("observations", observations.join("|")),
    fact("first-result", firstResult),
    fact("second-result", secondResult),
    fact("detail", JSON.stringify(detail)),
    fact("trusted", event.isTrusted),
  ];
}

const SCENARIOS: Record<string, EventScenario> = {
  "capture-bubble-order": captureBubbleOrder,
  "propagation-stop-boundaries": propagationStopBoundaries,
  "listener-options-signal": listenerOptionsSignal,
  "shadow-composed-retarget": shadowComposedRetarget,
  "closed-shadow-path": closedShadowPath,
  "slot-reassignment-path": slotReassignmentPath,
  "focus-blur-order": focusBlurOrder,
  "shadow-focus-retarget": shadowFocusRetarget,
  "listener-mutation-dispatch": listenerMutationDispatch,
  "custom-event-redispatch": customEventRedispatch,
};

export async function runEventShadowFocusBoundaryCase(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformBoundaryResult> {
  const scenario = SCENARIOS[spec.slug];
  assertFixture(scenario, `missing event/shadow/focus scenario ${spec.slug}`);
  const facts = await scenario(host, meta, spec, capture);
  return { status: "ready", facts };
}

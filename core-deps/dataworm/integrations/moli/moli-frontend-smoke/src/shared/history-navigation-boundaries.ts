import { assertFixture, microtaskTurns } from "./harness";
import {
  capturePlatformStep,
  fact,
  type CapturePlatformFrame,
  type PlatformBoundaryResult,
  type PlatformFact,
  withEventTimeout,
} from "./platform-boundary";
import type { CaseSpec, SmokeMeta } from "./types";

type HistoryScenario = (
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
) => Promise<PlatformFact[]>;

function scenarioRoot(host: HTMLElement, name: string): HTMLElement {
  const root = document.createElement("section");
  root.dataset.historyScenario = name;
  host.append(root);
  return root;
}

function output(root: HTMLElement, name: string, value: string): HTMLOutputElement {
  const element = document.createElement("output");
  element.dataset.historyOutput = name;
  element.textContent = value;
  root.append(element);
  return element;
}

function hashOf(url: string | null | undefined): string {
  if (!url) {
    return "(null)";
  }
  return new URL(url).hash || "(none)";
}

function currentPath(): string {
  return `${location.pathname}${location.search}${location.hash}`;
}

function nextHashChange(
  target: Window,
  expectedHash: string,
  trigger: () => void,
  label: string,
): Promise<HashChangeEvent> {
  return withEventTimeout(
    new Promise<HashChangeEvent>((resolve) => {
      const listener = (event: HashChangeEvent): void => {
        if (target.location.hash !== expectedHash) {
          return;
        }
        target.removeEventListener("hashchange", listener);
        resolve(event);
      };
      target.addEventListener("hashchange", listener);
      trigger();
    }),
    label,
  );
}

async function appendHistoryFrame(
  root: HTMLElement,
  title: string,
): Promise<HTMLIFrameElement> {
  const frame = document.createElement("iframe");
  frame.title = title;
  const loaded = withEventTimeout(
    new Promise<void>((resolve, reject) => {
      frame.addEventListener("load", () => resolve(), { once: true });
      frame.addEventListener(
        "error",
        () => reject(new Error(`${title} failed to load`)),
        { once: true },
      );
    }),
    `${title} load`,
  );
  frame.src = "/support/boundary-frame.html?scenario=history-child&phase=initial";
  root.append(frame);
  await loaded;
  assertFixture(frame.contentWindow, `${title} exposes contentWindow`);
  assertFixture(frame.contentDocument, `${title} exposes contentDocument`);
  return frame;
}

async function pushReplaceStateClone(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const initialLength = history.length;
  const original: {
    step: number;
    nested: { owner: string; seed: number };
    values: number[];
  } = {
    step: 1,
    nested: { owner: meta.framework, seed: spec.seed },
    values: [spec.variant, spec.variant + 1, spec.variant + 2],
  };
  history.pushState(
    original,
    "ignored push title",
    `?history=push-${spec.variant}#alpha`,
  );
  original.nested.owner = "mutated-after-push";
  original.values.push(99);
  const pushed = history.state as {
    step: number;
    nested: { owner: string; seed: number };
    values: number[];
  };
  assertFixture(pushed !== original, "pushState stored a cloned top-level value");
  assertFixture(pushed.nested !== original.nested, "pushState cloned nested values");
  assertFixture(pushed.nested.owner === meta.framework, "later source mutation did not alter state");
  assertFixture(pushed.values.length === 3, "later source array mutation did not alter state");
  output(
    root,
    "pushed",
    `${currentPath()}\n${pushed.step}:${pushed.nested.owner}:${pushed.nested.seed}\n${pushed.values.join("|")}`,
  );
  await capturePlatformStep(host, capture, "platform-1", "history-pushed", [
    location.search,
    location.hash,
    history.length - initialLength,
    pushed.nested.owner,
    pushed.values.length,
  ]);

  const replacement = {
    step: 2,
    nested: { owner: `${meta.framework}-replacement`, seed: spec.seed + 1 },
    values: [spec.variant + 4, spec.variant + 5],
  };
  history.replaceState(
    replacement,
    "ignored replacement title",
    `?history=replace-${spec.seed}#beta`,
  );
  const replaced = history.state as typeof replacement;
  assertFixture(replaced !== replacement, "replaceState stored a cloned value");
  assertFixture(history.length === initialLength + 1, "replaceState reused the pushed entry");
  assertFixture(document.title === meta.title, "history title arguments did not change document.title");
  output(
    root,
    "replaced",
    `${currentPath()}\n${replaced.step}:${replaced.nested.owner}:${replaced.nested.seed}\n${replaced.values.join("|")}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "history-replaced", [
    location.search,
    location.hash,
    history.length - initialLength,
    replaced.nested.owner,
    document.title === meta.title,
  ]);

  return [
    fact("length-delta", history.length - initialLength),
    fact("state-step", replaced.step),
    fact("state-owner", replaced.nested.owner),
    fact("source-isolated", pushed.values.length === 3),
    fact("path", currentPath()),
  ];
}

async function backForwardEventOrder(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const order: string[] = [];
  const onPopState = (event: PopStateEvent): void => {
    order.push(`pop:${String((event.state as { step?: number } | null)?.step ?? "null")}`);
    queueMicrotask(() => order.push(`pop-micro:${location.hash}`));
  };
  const onHashChange = (event: HashChangeEvent): void => {
    order.push(`hash:${hashOf(event.oldURL)}>${hashOf(event.newURL)}`);
    queueMicrotask(() => order.push(`hash-micro:${location.hash}`));
  };
  addEventListener("popstate", onPopState);
  addEventListener("hashchange", onHashChange);
  try {
    history.replaceState({ step: 0 }, "", "#base");
    history.pushState({ step: 1 }, "", "#one");
    history.pushState({ step: 2 }, "", "#two");
    output(root, "stack", `${location.hash}\n${String(history.state.step)}\nentries=base|one|two`);
    await capturePlatformStep(host, capture, "platform-1", "history-stack-built", [
      location.hash,
      history.state.step,
      order.length,
    ]);

    const backEvent = await nextHashChange(window, "#one", () => history.back(), "history back");
    await microtaskTurns();
    const backOrder = order.join("|");
    assertFixture(history.state.step === 1, "back restored the prior history state");
    assertFixture(hashOf(backEvent.oldURL) === "#two", "back oldURL described the later entry");
    assertFixture(hashOf(backEvent.newURL) === "#one", "back newURL described the restored entry");
    output(root, "back", `${location.hash}\nstate=${history.state.step}\n${backOrder}`);
    await capturePlatformStep(host, capture, "platform-2", "history-back-complete", [
      location.hash,
      history.state.step,
      backOrder,
    ]);

    order.length = 0;
    const forwardEvent = await nextHashChange(
      window,
      "#two",
      () => history.forward(),
      "history forward",
    );
    await microtaskTurns();
    const forwardOrder = order.join("|");
    assertFixture(history.state.step === 2, "forward restored the later history state");
    output(root, "forward", `${location.hash}\nstate=${history.state.step}\n${forwardOrder}`);
    return [
      fact("back-order", backOrder),
      fact("forward-order", forwardOrder),
      fact("forward-old", hashOf(forwardEvent.oldURL)),
      fact("forward-new", hashOf(forwardEvent.newURL)),
      fact("final-state", history.state.step),
    ];
  } finally {
    removeEventListener("popstate", onPopState);
    removeEventListener("hashchange", onHashChange);
  }
}

async function goMultiEntryTraversal(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const restored: string[] = [];
  const onPopState = (event: PopStateEvent): void => {
    restored.push(`${location.hash}:${String((event.state as { index?: number } | null)?.index ?? "null")}`);
  };
  addEventListener("popstate", onPopState);
  try {
    history.replaceState({ index: 0 }, "", "#zero");
    for (let index = 1; index <= 3; index += 1) {
      history.pushState({ index, label: `entry-${index}` }, "", `#entry-${index}`);
    }
    output(root, "go-stack", `${location.hash}\nstate=${history.state.index}\ncount=4`);
    await capturePlatformStep(host, capture, "platform-1", "history-go-stack", [
      location.hash,
      history.state.index,
      4,
    ]);

    await nextHashChange(
      window,
      "#entry-1",
      () => history.go(-2),
      "history.go negative traversal",
    );
    assertFixture(history.state.index === 1, "history.go(-2) restored entry one");
    output(root, "go-back", `${location.hash}\nstate=${history.state.index}\n${restored.join("|")}`);
    await capturePlatformStep(host, capture, "platform-2", "history-go-negative", [
      location.hash,
      history.state.index,
      restored.join("|"),
    ]);

    await nextHashChange(
      window,
      "#entry-2",
      () => history.go(1),
      "history.go positive traversal",
    );
    assertFixture(history.state.index === 2, "history.go(1) restored entry two");
    output(root, "go-forward", `${location.hash}\nstate=${history.state.index}\n${restored.join("|")}`);
    return [
      fact("restored", restored.join("|")),
      fact("final-hash", location.hash),
      fact("final-index", history.state.index),
      fact("forward-available", window.navigation.canGoForward),
    ];
  } finally {
    removeEventListener("popstate", onPopState);
  }
}

async function fragmentAnchorTarget(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const firstId = `section-${spec.variant}-café`;
  const secondId = `section-${spec.variant}-東京`;
  const anchor = document.createElement("a");
  anchor.href = `#${encodeURIComponent(firstId)}`;
  anchor.textContent = "Open first fragment";
  const first = document.createElement("article");
  first.id = firstId;
  first.textContent = "First fragment target";
  const second = document.createElement("article");
  second.id = secondId;
  second.textContent = "Second fragment target";
  root.append(anchor, first, second);

  const events: string[] = [];
  const onPopState = (event: PopStateEvent): void => {
    events.push(`pop:${String(event.state)}`);
  };
  const onHashChange = (event: HashChangeEvent): void => {
    events.push(`hash:${hashOf(event.oldURL)}>${hashOf(event.newURL)}`);
  };
  addEventListener("popstate", onPopState);
  addEventListener("hashchange", onHashChange);
  try {
    const firstHash = `#${encodeURIComponent(firstId)}`;
    await nextHashChange(window, firstHash, () => anchor.click(), "fragment anchor click");
    const firstTarget = document.querySelector(":target");
    assertFixture(firstTarget === first, "anchor activation selected the first :target");
    output(root, "anchor-target", `${location.hash}\n${firstTarget.id}\n${events.join("|")}`);
    await capturePlatformStep(host, capture, "platform-1", "fragment-anchor-target", [
      location.hash,
      firstTarget.id,
      events.join("|"),
    ]);

    const secondHash = `#${encodeURIComponent(secondId)}`;
    await nextHashChange(
      window,
      secondHash,
      () => location.replace(secondHash),
      "fragment location.replace",
    );
    const secondTarget = document.querySelector(":target");
    assertFixture(secondTarget === second, "location.replace selected the second :target");
    output(root, "replace-target", `${location.hash}\n${secondTarget.id}\n${events.join("|")}`);
    await capturePlatformStep(host, capture, "platform-2", "fragment-replace-target", [
      location.hash,
      secondTarget.id,
      events.join("|"),
    ]);

    return [
      fact("first-target", first.id),
      fact("second-target", second.id),
      fact("events", events.join("|")),
      fact("current-target", (document.querySelector(":target") as Element | null)?.id ?? "missing"),
    ];
  } finally {
    removeEventListener("popstate", onPopState);
    removeEventListener("hashchange", onHashChange);
  }
}

async function stateCloneSecurityErrors(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const sourceBuffer = new Uint8Array([1, spec.variant + 2, 255]).buffer;
  const source = {
    map: new Map<string, number>([
      ["seed", spec.seed],
      ["variant", spec.variant],
    ]),
    set: new Set([meta.framework, "stable"]),
    date: new Date("2024-03-04T05:06:07.000Z"),
    buffer: sourceBuffer,
    bigint: 9007199254740993n + BigInt(spec.variant),
  };
  history.pushState(source, "", "#rich-state");
  const stored = history.state as typeof source;
  assertFixture(stored.map instanceof Map, "history state retained Map branding");
  assertFixture(stored.set instanceof Set, "history state retained Set branding");
  assertFixture(stored.date instanceof Date, "history state retained Date branding");
  assertFixture(stored.buffer instanceof ArrayBuffer, "history state retained ArrayBuffer branding");
  assertFixture(stored.map !== source.map, "history state cloned Map identity");
  assertFixture(stored.buffer !== sourceBuffer, "history state cloned ArrayBuffer identity");
  const bytes = Array.from(new Uint8Array(stored.buffer)).join("|");
  output(
    root,
    "rich-state",
    `${[...stored.map].map(([key, value]) => `${key}:${value}`).join("|")}\n${[...stored.set].join("|")}\n${stored.date.toISOString()}\n${bytes}\n${String(stored.bigint)}`,
  );
  await capturePlatformStep(host, capture, "platform-1", "history-rich-state", [
    stored.map.size,
    stored.set.size,
    stored.date.toISOString(),
    bytes,
    String(stored.bigint),
  ]);

  let cloneError = "missing";
  try {
    history.pushState({ callback: () => spec.seed }, "", "#invalid-clone");
  } catch (error: unknown) {
    cloneError = error instanceof DOMException ? error.name : "non-dom-exception";
  }
  let securityError = "missing";
  try {
    history.replaceState({ crossOrigin: true }, "", "https://example.invalid/elsewhere");
  } catch (error: unknown) {
    securityError = error instanceof DOMException ? error.name : "non-dom-exception";
  }
  assertFixture(cloneError === "DataCloneError", "uncloneable history state threw DataCloneError");
  assertFixture(securityError === "SecurityError", "cross-origin history URL threw SecurityError");
  assertFixture(location.hash === "#rich-state", "failed history operations kept the active URL");
  assertFixture(history.state.map instanceof Map, "failed operations kept the active state");
  output(root, "history-errors", `${cloneError}\n${securityError}\n${location.hash}\n${history.state.map.size}`);
  await capturePlatformStep(host, capture, "platform-2", "history-error-boundaries", [
    cloneError,
    securityError,
    location.hash,
    history.state.map.size,
  ]);

  return [
    fact("clone-error", cloneError),
    fact("security-error", securityError),
    fact("map", [...stored.map].map(([key, value]) => `${key}:${value}`).join("|")),
    fact("set", [...stored.set].join("|")),
    fact("buffer", bytes),
    fact("state-preserved", location.hash === "#rich-state"),
  ];
}

async function scrollRestorationTraversal(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const initial = history.scrollRestoration;
  history.replaceState({ step: 0 }, "", "#scroll-base");
  history.scrollRestoration = "manual";
  history.pushState({ step: 1 }, "", "#scroll-manual");
  assertFixture(history.scrollRestoration === "manual", "scrollRestoration accepted manual");
  output(root, "manual", `${initial}>${history.scrollRestoration}\n${location.hash}\n${history.state.step}`);
  await capturePlatformStep(host, capture, "platform-1", "scroll-restoration-manual", [
    initial,
    history.scrollRestoration,
    location.hash,
    history.state.step,
  ]);

  await nextHashChange(
    window,
    "#scroll-base",
    () => history.back(),
    "scroll restoration back",
  );
  const restoredMode = history.scrollRestoration;
  history.scrollRestoration = "auto";
  assertFixture(restoredMode === "manual", "traversal retained manual scroll restoration");
  assertFixture(history.scrollRestoration === "auto", "scrollRestoration returned to auto");
  output(
    root,
    "restored",
    `${restoredMode}>${history.scrollRestoration}\n${location.hash}\n${history.state.step}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "scroll-restoration-traversed", [
    restoredMode,
    history.scrollRestoration,
    location.hash,
    history.state.step,
  ]);

  return [
    fact("initial-mode", initial),
    fact("traversal-mode", restoredMode),
    fact("final-mode", history.scrollRestoration),
    fact("restored-state", history.state.step),
    fact("variant", spec.variant),
  ];
}

async function navigationCurrentEntryState(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const navigation = window.navigation;
  assertFixture(navigation.currentEntry, "Navigation exposes a current entry");
  const changes: string[] = [];
  navigation.addEventListener("currententrychange", (event) => {
    changes.push(
      `${event.navigationType}:${event.from ? hashOf(event.from.url) : "(null)"}>${navigation.currentEntry ? hashOf(navigation.currentEntry.url) : "(null)"}`,
    );
  });
  const source = {
    owner: meta.framework,
    nested: { seed: spec.seed },
    values: [spec.variant, spec.variant + 1],
  };
  navigation.updateCurrentEntry({ state: source });
  source.nested.seed = -1;
  source.values.push(99);
  const initialEntry = navigation.currentEntry;
  const initialState = initialEntry.getState() as {
    owner: string;
    nested: { seed: number };
    values: number[];
  };
  assertFixture(initialState !== source, "Navigation entry state cloned the source object");
  assertFixture(initialState.nested.seed === spec.seed, "Navigation entry state isolated nested mutation");
  assertFixture(initialState.values.length === 2, "Navigation entry state isolated array mutation");
  output(
    root,
    "current-entry",
    `${hashOf(initialEntry.url)}\n${initialState.owner}:${initialState.nested.seed}\n${initialState.values.join("|")}\n${changes.join("|")}`,
  );
  await capturePlatformStep(host, capture, "platform-1", "navigation-current-state", [
    hashOf(initialEntry.url),
    initialState.owner,
    initialState.nested.seed,
    initialState.values.length,
    changes.join("|"),
  ]);

  const targetHash = `#navigation-state-${spec.variant}`;
  const hashChanged = nextHashChange(
    window,
    targetHash,
    () => undefined,
    "Navigation.navigate hashchange",
  );
  const result = navigation.navigate(targetHash, {
    history: "push",
    state: { owner: `${meta.framework}-next`, step: 2 },
    info: { source: spec.slug },
  });
  const [committed, finished] = await Promise.all([
    result.committed,
    result.finished,
    hashChanged,
  ]);
  const nextState = navigation.currentEntry?.getState() as {
    owner: string;
    step: number;
  };
  assertFixture(committed === navigation.currentEntry, "committed resolved to currentEntry");
  assertFixture(finished === navigation.currentEntry, "finished resolved to currentEntry");
  assertFixture(nextState.owner === `${meta.framework}-next`, "destination retained navigation state");
  assertFixture(history.state === null, "Navigation state stayed independent from history.state");
  output(
    root,
    "next-entry",
    `${location.hash}\n${nextState.owner}:${nextState.step}\nhistory=${String(history.state)}\n${changes.join("|")}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "navigation-next-state", [
    location.hash,
    nextState.owner,
    nextState.step,
    String(history.state),
    changes.join("|"),
  ]);

  return [
    fact("initial-owner", initialState.owner),
    fact("next-owner", nextState.owner),
    fact("history-state", String(history.state)),
    fact("entry-replaced", initialEntry !== navigation.currentEntry),
    fact("changes", changes.join("|")),
    fact("can-go-back", navigation.canGoBack),
  ];
}

async function navigationInterceptLifecycle(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const navigation = window.navigation;
  const order: string[] = [];
  let capturedDestination = "missing";
  let capturedState = "missing";
  let capturedInfo = "missing";
  const targetHash = `#intercept-${spec.variant}`;
  const onNavigate = (event: NavigateEvent): void => {
    if (hashOf(event.destination.url) !== targetHash) {
      return;
    }
    order.push(`navigate:${event.navigationType}:${event.hashChange}`);
    capturedDestination = hashOf(event.destination.url);
    capturedState = JSON.stringify(event.destination.getState());
    capturedInfo = JSON.stringify(event.info);
    assertFixture(event.canIntercept, "same-document navigation can be intercepted");
    event.intercept({
      focusReset: "manual",
      scroll: "manual",
      handler: async () => {
        order.push("handler:start");
        output(
          root,
          "intercept-start",
          `${capturedDestination}\n${capturedState}\n${capturedInfo}\n${order.join("|")}`,
        );
        await capturePlatformStep(host, capture, "platform-1", "navigation-intercept-start", [
          capturedDestination,
          capturedState,
          capturedInfo,
          order.join("|"),
        ]);
        await Promise.resolve();
        order.push("handler:microtask");
        root.dataset.interceptOwner = meta.framework;
      },
    });
  };
  navigation.addEventListener("navigate", onNavigate);
  try {
    const result = navigation.navigate(targetHash, {
      history: "push",
      state: { step: spec.seed, owner: meta.framework },
      info: { action: "fixture-intercept", variant: spec.variant },
    });
    assertFixture(result.committed && result.finished, "navigation result exposes both promises");
    result.committed.then(() => order.push("committed"));
    await result.finished;
    order.push("finished");
    assertFixture(location.hash === targetHash, "intercepted navigation committed its fragment");
    assertFixture(root.dataset.interceptOwner === meta.framework, "intercept handler mutated author DOM");
    output(root, "intercept-finished", `${location.hash}\n${order.join("|")}\ntransition=${String(navigation.transition)}`);
    await capturePlatformStep(host, capture, "platform-2", "navigation-intercept-finished", [
      location.hash,
      order.join("|"),
      String(navigation.transition),
      navigation.currentEntry?.sameDocument,
    ]);

    return [
      fact("destination", capturedDestination),
      fact("state", capturedState),
      fact("info", capturedInfo),
      fact("order", order.join("|")),
      fact("transition-cleared", navigation.transition === null),
      fact("same-document", navigation.currentEntry?.sameDocument),
    ];
  } finally {
    navigation.removeEventListener("navigate", onNavigate);
  }
}

async function navigationTraversePromises(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const navigation = window.navigation;
  history.replaceState({ step: 0 }, "", "#traverse-base");
  history.pushState({ step: 1 }, "", "#traverse-one");
  history.pushState({ step: 2 }, "", "#traverse-two");
  const target = navigation.entries().find((entry) => hashOf(entry.url) === "#traverse-one");
  assertFixture(target, "Navigation.entries exposed the prior fragment entry");
  assertFixture(target.key.length > 0, "prior Navigation entry exposed a key");
  output(
    root,
    "traverse-stack",
    `${navigation.entries().map((entry) => hashOf(entry.url)).join("|")}\ncurrent=${location.hash}\nback=${navigation.canGoBack}:forward=${navigation.canGoForward}`,
  );
  await capturePlatformStep(host, capture, "platform-1", "navigation-traverse-stack", [
    navigation.entries().map((entry) => hashOf(entry.url)).join("|"),
    location.hash,
    navigation.canGoBack,
    navigation.canGoForward,
  ]);

  const order: string[] = [];
  navigation.addEventListener("currententrychange", (event) => {
    order.push(`entry:${event.navigationType}:${event.from ? hashOf(event.from.url) : "(null)"}`);
    queueMicrotask(() => order.push(`entry-micro:${location.hash}`));
  });
  addEventListener("popstate", (event) => {
    order.push(`pop:${String((event.state as { step?: number } | null)?.step ?? "null")}`);
    queueMicrotask(() => order.push(`pop-micro:${location.hash}`));
  });
  addEventListener("hashchange", () => {
    order.push(`hash:${location.hash}`);
    queueMicrotask(() => order.push(`hash-micro:${location.hash}`));
  });
  const hashChanged = nextHashChange(
    window,
    "#traverse-one",
    () => undefined,
    "Navigation.traverseTo hashchange",
  );
  const before = navigation.currentEntry;
  const result = navigation.traverseTo(target.key, { info: { variant: spec.variant } });
  assertFixture(result.committed && result.finished, "traversal result exposes both promises");
  const committedPromise = result.committed.then((entry) => {
    order.push(`committed:${hashOf(entry.url)}:${entry === navigation.currentEntry}`);
    return entry;
  });
  const finishedPromise = result.finished.then((entry) => {
    order.push(`finished:${hashOf(entry.url)}:${entry === navigation.currentEntry}`);
    return entry;
  });
  const [committed, finished] = await Promise.all([
    committedPromise,
    finishedPromise,
    hashChanged,
  ]);
  await microtaskTurns();
  assertFixture(committed === navigation.currentEntry, "traversal committed to currentEntry");
  assertFixture(finished === navigation.currentEntry, "traversal finished at currentEntry");
  assertFixture(before !== navigation.currentEntry, "traversal replaced the current entry object");
  assertFixture(history.state.step === 1, "traversal restored History state");
  output(
    root,
    "traverse-restored",
    `${location.hash}\nstate=${history.state.step}\n${order.join("|")}\nback=${navigation.canGoBack}:forward=${navigation.canGoForward}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "navigation-traverse-restored", [
    location.hash,
    history.state.step,
    order.join("|"),
    navigation.canGoBack,
    navigation.canGoForward,
  ]);

  return [
    fact("order", order.join("|")),
    fact("state", history.state.step),
    fact("current-hash", hashOf(navigation.currentEntry?.url ?? location.href)),
    fact("entry-changed", before !== navigation.currentEntry),
    fact("can-go-back", navigation.canGoBack),
    fact("can-go-forward", navigation.canGoForward),
  ];
}

async function childContextHistory(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const frame = await appendHistoryFrame(root, `history-child-${meta.framework}-${spec.variant}`);
  const child = frame.contentWindow;
  const childDocument = frame.contentDocument;
  assertFixture(child, "history child window exists");
  assertFixture(childDocument, "history child document exists");
  const childRoot = childDocument.querySelector("#frame-root");
  assertFixture(childRoot, "history child root exists");

  child.history.replaceState({ step: 0 }, "", "#child-base");
  child.history.pushState({ step: 1 }, "", "#child-one");
  child.history.pushState({ step: 2 }, "", "#child-two");
  const childInitial = childDocument.createElement("output");
  childInitial.id = "child-history-stack";
  childInitial.textContent = `${child.location.hash}:state=${child.history.state.step}:length=${child.history.length}`;
  childRoot.append(childInitial);
  output(root, "parent-before", `${location.hash || "(none)"}\nchild=${child.location.hash}`);
  await capturePlatformStep(host, capture, "platform-1", "child-history-stack", [
    child.location.hash,
    child.history.state.step,
    child.history.length,
    location.hash || "(none)",
  ]);

  const childEvents: string[] = [];
  child.addEventListener("popstate", (event) => {
    childEvents.push(`pop:${String((event.state as { step?: number } | null)?.step ?? "null")}`);
  });
  child.addEventListener("hashchange", (event) => {
    childEvents.push(`hash:${hashOf(event.oldURL)}>${hashOf(event.newURL)}`);
  });
  await nextHashChange(
    child,
    "#child-one",
    () => child.history.back(),
    "child history back",
  );
  assertFixture(child.history.state.step === 1, "child back restored child state");
  assertFixture(!location.hash, "child traversal did not change parent location");
  const childBack = childDocument.createElement("output");
  childBack.id = "child-history-back";
  childBack.textContent = `${child.location.hash}:state=${child.history.state.step}:${childEvents.join("|")}`;
  childRoot.append(childBack);
  output(root, "parent-after-back", `${location.hash || "(none)"}\nchild=${child.location.hash}\n${childEvents.join("|")}`);
  await capturePlatformStep(host, capture, "platform-2", "child-history-back", [
    child.location.hash,
    child.history.state.step,
    childEvents.join("|"),
    location.hash || "(none)",
  ]);

  await nextHashChange(
    child,
    "#child-two",
    () => child.history.forward(),
    "child history forward",
  );
  const childForward = childDocument.createElement("output");
  childForward.id = "child-history-forward";
  childForward.textContent = `${child.location.hash}:state=${child.history.state.step}:${childEvents.join("|")}`;
  childRoot.append(childForward);
  return [
    fact("parent-hash", location.hash || "(none)"),
    fact("child-hash", child.location.hash),
    fact("child-state", child.history.state.step),
    fact("child-events", childEvents.join("|")),
    fact("realm-isolated", child.history !== history),
    fact("document-isolated", childDocument !== document),
  ];
}

const SCENARIOS: Record<string, HistoryScenario> = {
  "push-replace-state-clone": pushReplaceStateClone,
  "back-forward-event-order": backForwardEventOrder,
  "go-multi-entry-traversal": goMultiEntryTraversal,
  "fragment-anchor-target": fragmentAnchorTarget,
  "state-clone-security-errors": stateCloneSecurityErrors,
  "scroll-restoration-traversal": scrollRestorationTraversal,
  "navigation-current-entry-state": navigationCurrentEntryState,
  "navigation-intercept-lifecycle": navigationInterceptLifecycle,
  "navigation-traverse-promises": navigationTraversePromises,
  "child-context-history": childContextHistory,
};

export async function runHistoryNavigationBoundaryCase(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformBoundaryResult> {
  const scenario = SCENARIOS[spec.slug];
  assertFixture(scenario, `missing history/navigation scenario ${spec.slug}`);
  const facts = await scenario(host, meta, spec, capture);
  return { status: "ready", facts };
}

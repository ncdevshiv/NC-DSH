import { assertFixture, microtaskTurns } from "./harness";
import {
  capturePlatformStep,
  errorName,
  fact,
  withEventTimeout,
  type CapturePlatformFrame,
  type PlatformBoundaryResult,
  type PlatformFact,
} from "./platform-boundary";
import type { CaseSpec, SmokeMeta } from "./types";

type ScriptScenario = (
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
) => Promise<PlatformFact[]>;

interface ScriptLifecycleMessage {
  source: "moli-script-lifecycle";
  phase: string;
  order?: string;
  value?: string;
}

function scenarioRoot(host: HTMLElement, name: string): HTMLElement {
  const root = document.createElement("section");
  root.dataset.scriptScenario = name;
  host.append(root);
  return root;
}

function output(root: HTMLElement, name: string, value: string): HTMLOutputElement {
  const element = document.createElement("output");
  element.dataset.scriptOutput = name;
  element.textContent = value;
  root.append(element);
  return element;
}

function scenarioToken(meta: SmokeMeta, spec: CaseSpec): string {
  return `p34-${meta.framework}-${spec.slug}-${spec.seed}`;
}

function scriptUrl(pathname: string, values: Record<string, string>): string {
  return `${pathname}?${new URLSearchParams(values)}`;
}

function htmlAttribute(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll('"', "&quot;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

function inlineJson(value: unknown): string {
  return JSON.stringify(value).replaceAll("<", "\\u003c");
}

function frameLoad(frame: HTMLIFrameElement, label: string): Promise<void> {
  return withEventTimeout(
    new Promise<void>((resolve) => {
      frame.addEventListener("load", () => resolve(), { once: true });
    }),
    label,
  );
}

function scriptCompletion(script: HTMLScriptElement, label: string): Promise<void> {
  return withEventTimeout(
    new Promise<void>((resolve, reject) => {
      script.addEventListener("load", () => resolve(), { once: true });
      script.addEventListener("error", () => reject(new Error(`${label} failed`)), {
        once: true,
      });
    }),
    label,
  );
}

function waitForScriptMessage(
  source: Window,
  phase: string,
  label: string,
): Promise<ScriptLifecycleMessage> {
  return withEventTimeout(
    new Promise<ScriptLifecycleMessage>((resolve) => {
      const listener = (event: MessageEvent<unknown>): void => {
        const data = event.data as Partial<ScriptLifecycleMessage> | null;
        if (
          event.source !== source ||
          data?.source !== "moli-script-lifecycle" ||
          data.phase !== phase
        ) {
          return;
        }
        window.removeEventListener("message", listener);
        resolve(data as ScriptLifecycleMessage);
      };
      window.addEventListener("message", listener);
    }),
    label,
  );
}

async function parserBlockingDeferOrder(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = scenarioToken(meta, spec);
  const deferUrl = scriptUrl("/support/scripts/classic.js", {
    token,
    label: "defer",
  });
  const dynamicUrl = scriptUrl("/support/scripts/classic.js", {
    token,
    label: "dynamic-after-load",
  });
  const frame = document.createElement("iframe");
  frame.dataset.scriptFrame = "parser-defer";
  root.append(frame);
  assertFixture(frame.contentWindow, "parser/defer frame has a Window");
  const dcl = waitForScriptMessage(
    frame.contentWindow,
    "dcl",
    "parser/defer DOMContentLoaded",
  );
  const loaded = frameLoad(frame, "parser/defer frame load");
  frame.srcdoc = `<!doctype html><html><head><meta charset="utf-8"><title>parser defer</title></head><body><main data-script-target><output id="order"></output></main><script>globalThis.__scriptLifecycleOrder=["inline-head"];document.querySelector("#order").textContent=globalThis.__scriptLifecycleOrder.join("|");<\/script><script defer src="${htmlAttribute(deferUrl)}"><\/script><script>globalThis.__scriptLifecycleOrder.push("inline-tail");document.addEventListener("DOMContentLoaded",()=>{globalThis.__scriptLifecycleOrder.push("dcl");document.querySelector("#order").textContent=globalThis.__scriptLifecycleOrder.join("|");parent.postMessage({source:"moli-script-lifecycle",phase:"dcl",order:globalThis.__scriptLifecycleOrder.join("|")},"*");});addEventListener("load",()=>{globalThis.__scriptLifecycleOrder.push("load");document.querySelector("#order").textContent=globalThis.__scriptLifecycleOrder.join("|");});<\/script></body></html>`;

  const dclMessage = await dcl;
  assertFixture(
    dclMessage.order === "inline-head|inline-tail|classic:defer|dcl",
    "defer script ran after parsing and before DOMContentLoaded",
  );
  output(root, "parser-dcl", dclMessage.order ?? "missing");
  await capturePlatformStep(host, capture, "platform-1", "parser-defer-dcl", [
    dclMessage.order,
  ]);

  await loaded;
  const frameDocument = frame.contentDocument;
  assertFixture(frameDocument, "parser/defer frame exposes its document");
  const dynamic = frameDocument.createElement("script");
  dynamic.src = dynamicUrl;
  dynamic.dataset.scriptId = "dynamic-frame-script";
  const dynamicLoaded = scriptCompletion(dynamic, "dynamic script after frame load");
  frameDocument.body.append(dynamic);
  await dynamicLoaded;
  const finalOrder = (frame.contentWindow as Window & { __scriptLifecycleOrder?: string[] })
    .__scriptLifecycleOrder?.join("|") ?? "missing";
  assertFixture(
    finalOrder ===
      "inline-head|inline-tail|classic:defer|dcl|load|classic:dynamic-after-load",
    "dynamic classic script extended the parser/defer order",
  );
  output(root, "parser-final", finalOrder);
  await capturePlatformStep(host, capture, "platform-2", "parser-defer-dynamic", [
    finalOrder,
    frameDocument.querySelectorAll("[data-classic-script]").length,
  ]);

  return [
    fact("dcl-order", dclMessage.order ?? "missing"),
    fact("final-order", finalOrder),
    fact("classic-markers", frameDocument.querySelectorAll("[data-classic-script]").length),
  ];
}

async function dynamicClassicSequentialQueue(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  root.dataset.scriptTarget = "";
  const token = scenarioToken(meta, spec);
  const runtime = globalThis as typeof globalThis & { __scriptLifecycleOrder?: string[] };
  runtime.__scriptLifecycleOrder = [];
  const alpha = document.createElement("script");
  alpha.async = false;
  alpha.src = scriptUrl("/support/scripts/classic.js", { token, label: "alpha" });
  alpha.dataset.scriptId = "alpha";
  const beta = document.createElement("script");
  beta.async = false;
  beta.src = scriptUrl("/support/scripts/classic.js", { token, label: "beta" });
  beta.dataset.scriptId = "beta";
  const alphaLoaded = scriptCompletion(alpha, "ordered alpha script");
  const betaLoaded = scriptCompletion(beta, "ordered beta script");

  output(root, "queued", `async=${alpha.async},${beta.async}|order=queued`);
  await capturePlatformStep(host, capture, "platform-1", "classic-scripts-queued", [
    alpha.async,
    beta.async,
    runtime.__scriptLifecycleOrder.length,
  ]);

  root.append(alpha, beta);
  await Promise.all([alphaLoaded, betaLoaded]);
  const order = runtime.__scriptLifecycleOrder.join("|");
  assertFixture(order === "classic:alpha|classic:beta", "async=false scripts kept insertion order");
  output(root, "executed", order);
  await capturePlatformStep(host, capture, "platform-2", "classic-scripts-executed", [
    order,
    root.querySelectorAll("[data-classic-script]").length,
  ]);

  delete runtime.__scriptLifecycleOrder;
  return [
    fact("order", order),
    fact("markers", root.querySelectorAll("[data-classic-script]").length),
    fact("async-flags", `${alpha.async}|${beta.async}`),
  ];
}

async function inertTypeCloneExecution(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  root.dataset.scriptTarget = "";
  const token = scenarioToken(meta, spec);
  const runtime = globalThis as typeof globalThis & { __scriptLifecycleOrder?: string[] };
  runtime.__scriptLifecycleOrder = [];
  const inert = document.createElement("script");
  inert.type = "application/x-moli-inert";
  inert.src = scriptUrl("/support/scripts/classic.js", { token, label: "clone" });
  inert.dataset.scriptId = "inert-original";
  root.append(inert);
  await microtaskTurns(3);
  inert.type = "text/javascript";
  await microtaskTurns(3);
  const before = runtime.__scriptLifecycleOrder.join("|") || "none";
  assertFixture(before === "none", "mutating an already inserted inert script did not execute it");
  output(root, "inert", before);
  await capturePlatformStep(host, capture, "platform-1", "inert-script-stayed-inert", [
    before,
    inert.type,
  ]);

  const executable = inert.cloneNode(true) as HTMLScriptElement;
  executable.removeAttribute("type");
  executable.dataset.scriptId = "executable-clone";
  const executed = scriptCompletion(executable, "cloned executable script");
  root.append(executable);
  await executed;
  executable.remove();
  root.append(executable);
  await microtaskTurns(3);
  const after = runtime.__scriptLifecycleOrder.join("|");
  assertFixture(after === "classic:clone", "cloned script executed exactly once across reinsert");
  output(root, "clone", after);
  await capturePlatformStep(host, capture, "platform-2", "script-clone-executed-once", [
    after,
    root.querySelectorAll("[data-classic-script]").length,
  ]);

  delete runtime.__scriptLifecycleOrder;
  return [fact("before", before), fact("after", after), fact("script-count", 2)];
}

async function documentWriteNestedScripts(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = scenarioToken(meta, spec);
  const writtenUrl = scriptUrl("/support/scripts/classic.js", { token, label: "written" });
  const afterUrl = scriptUrl("/support/scripts/classic.js", { token, label: "after-write" });
  const frame = document.createElement("iframe");
  frame.dataset.scriptFrame = "document-write";
  root.append(frame);
  assertFixture(frame.contentWindow, "document.write frame has a Window");
  const ready = waitForScriptMessage(
    frame.contentWindow,
    "write-ready",
    "nested document.write scripts",
  );
  frame.srcdoc = `<!doctype html><html><head><meta charset="utf-8"><title>write scripts</title></head><body><main data-script-target></main><script>globalThis.__scriptLifecycleOrder=["outer"];document.write("<section id='written-section'>written</section>");document.write("<script>globalThis.__scriptLifecycleOrder.push('nested');document.querySelector('#written-section').dataset.nested='yes';<\\/script>");document.write("<script src='${htmlAttribute(writtenUrl)}'><\\/script>");document.addEventListener("DOMContentLoaded",()=>parent.postMessage({source:"moli-script-lifecycle",phase:"write-ready",order:globalThis.__scriptLifecycleOrder.join("|")},"*"));<\/script></body></html>`;
  const message = await ready;
  assertFixture(
    message.order === "outer|nested|classic:written",
    `nested and external document.write scripts ran in parser order; actual=${message.order ?? "missing"}`,
  );
  output(root, "write-order", message.order ?? "missing");
  await capturePlatformStep(host, capture, "platform-1", "document-write-scripts", [
    message.order,
    frame.contentDocument?.querySelector("#written-section")?.getAttribute("data-nested"),
  ]);

  const frameDocument = frame.contentDocument;
  assertFixture(frameDocument, "document.write frame exposes its document");
  const dynamic = frameDocument.createElement("script");
  dynamic.src = afterUrl;
  dynamic.dataset.scriptId = "after-write";
  const complete = scriptCompletion(dynamic, "script after document.write parsing");
  frameDocument.body.append(dynamic);
  await complete;
  const finalOrder = (frame.contentWindow as Window & { __scriptLifecycleOrder?: string[] })
    .__scriptLifecycleOrder?.join("|") ?? "missing";
  output(root, "write-final", finalOrder);
  await capturePlatformStep(host, capture, "platform-2", "document-write-followup", [
    finalOrder,
    frameDocument.querySelectorAll("script").length,
  ]);

  return [
    fact("initial", message.order ?? "missing"),
    fact("final", finalOrder),
    fact("nested", frameDocument.querySelector("#written-section")?.getAttribute("data-nested")),
  ];
}

async function moduleStaticGraphTopLevelAwait(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = scenarioToken(meta, spec);
  const runtime = globalThis as typeof globalThis & { __scriptModuleOrder?: string[] };
  runtime.__scriptModuleOrder = [];
  const entryUrl = scriptUrl("/support/scripts/module-entry.js", { token, version: "v1" });
  const branchUrl = scriptUrl("/support/scripts/module-branch.js", { token, version: "v1" });
  const ready = withEventTimeout(
    new Promise<CustomEvent<{ branchValue: string; order: string }>>((resolve) => {
      globalThis.addEventListener(
        "smoke-module-ready",
        (event) => resolve(event as CustomEvent<{ branchValue: string; order: string }>),
        { once: true },
      );
    }),
    "static module graph ready event",
  );
  const script = document.createElement("script");
  script.type = "module";
  script.src = entryUrl;
  const loaded = scriptCompletion(script, "static module graph script");
  root.append(script);
  const event = await ready;
  await loaded;
  const first = `${event.detail.order}|${event.detail.branchValue}`;
  assertFixture(
    event.detail.order === "leaf:v1|branch|entry",
    "static module graph and top-level await completed in dependency order",
  );
  output(root, "module-graph", first);
  await capturePlatformStep(host, capture, "platform-1", "module-static-graph", [first]);

  const namespace = (await import(branchUrl)) as { branchValue: string };
  const finalOrder = runtime.__scriptModuleOrder.join("|");
  assertFixture(finalOrder === "leaf:v1|branch|entry", "dynamic import reused the document module map");
  output(root, "module-reimport", `${namespace.branchValue}|${finalOrder}`);
  await capturePlatformStep(host, capture, "platform-2", "module-graph-reused", [
    namespace.branchValue,
    finalOrder,
  ]);

  delete runtime.__scriptModuleOrder;
  return [fact("initial", first), fact("final-order", finalOrder), fact("branch", namespace.branchValue)];
}

async function dynamicImportCacheQueryIdentity(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = scenarioToken(meta, spec);
  const firstUrl = scriptUrl("/support/scripts/module-counter.js", { token, version: "v1" });
  const secondUrl = scriptUrl("/support/scripts/module-counter.js", { token, version: "v2" });
  const [first, same] = (await Promise.all([import(firstUrl), import(firstUrl)])) as Array<{
    count: number;
    value: string;
  }>;
  const firstSnapshot = `${first === same}|${first.count}|${first.value}`;
  assertFixture(first === same && first.count === 1, "same dynamic import URL reused namespace identity");
  output(root, "import-same", firstSnapshot);
  await capturePlatformStep(host, capture, "platform-1", "dynamic-import-same-url", [
    firstSnapshot,
  ]);

  const second = (await import(secondUrl)) as { count: number; value: string };
  const again = (await import(firstUrl)) as { count: number; value: string };
  const secondSnapshot = `${second.count}|${second.value}|${again.count}|${again === first}`;
  assertFixture(second.count === 1 && again.count === 1, "query-distinct module evaluated once per URL");
  output(root, "import-query", secondSnapshot);
  await capturePlatformStep(host, capture, "platform-2", "dynamic-import-query-identity", [
    secondSnapshot,
  ]);

  return [
    fact("same-url", firstSnapshot),
    fact("query-distinct", secondSnapshot),
    fact("namespace-identity", first === same && again === first),
  ];
}

async function importMapBareSpecifier(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = scenarioToken(meta, spec);
  const mappedUrl = scriptUrl("/support/scripts/module-map.js", { token });
  const frame = document.createElement("iframe");
  frame.dataset.scriptFrame = "import-map";
  root.append(frame);
  assertFixture(frame.contentWindow, "import-map frame has a Window");
  const phaseOne = waitForScriptMessage(frame.contentWindow, "map-1", "import map first phase");
  frame.srcdoc = `<!doctype html><html><head><meta charset="utf-8"><title>import map</title><script type="importmap">${inlineJson({ imports: { "smoke-package": mappedUrl } })}<\/script></head><body><main><output id="mapped"></output></main><script type="module">import { mappedValue } from "smoke-package";const target=document.querySelector("#mapped");target.textContent=mappedValue;parent.postMessage({source:"moli-script-lifecycle",phase:"map-1",value:mappedValue},"*");addEventListener("message",async(event)=>{if(event.data!=="continue-import-map")return;const same=await import("smoke-package");target.textContent=mappedValue+"|"+(same.mappedValue===mappedValue)+"|"+globalThis.__scriptModuleOrder.join("|");parent.postMessage({source:"moli-script-lifecycle",phase:"map-2",value:target.textContent},"*");},{once:true});<\/script></body></html>`;
  const first = await phaseOne;
  output(root, "map-first", first.value ?? "missing");
  await capturePlatformStep(host, capture, "platform-1", "import-map-static", [first.value]);

  const phaseTwo = waitForScriptMessage(frame.contentWindow, "map-2", "import map dynamic phase");
  frame.contentWindow.postMessage("continue-import-map", "*");
  const second = await phaseTwo;
  assertFixture(
    second.value === `mapped:${token}|true|mapped`,
    "static and dynamic bare imports shared the import-map module",
  );
  output(root, "map-second", second.value ?? "missing");
  await capturePlatformStep(host, capture, "platform-2", "import-map-dynamic", [second.value]);

  return [
    fact("static", first.value ?? "missing"),
    fact("dynamic", second.value ?? "missing"),
    fact("frame-output", frame.contentDocument?.querySelector("#mapped")?.textContent ?? "missing"),
  ];
}

async function moduleFailureRecovery(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = scenarioToken(meta, spec);
  const runtime = globalThis as typeof globalThis & { __scriptModuleOrder?: string[] };
  runtime.__scriptModuleOrder = [];
  const failingUrl = scriptUrl("/support/scripts/module-throw.js", { token });
  const recoveryUrl = scriptUrl("/support/scripts/module-recovery.js", { token });
  const failure = await import(failingUrl).then(
    () => "resolved",
    (error: unknown) => `${errorName(error)}:${error instanceof Error ? error.message : String(error)}`,
  );
  assertFixture(failure === `Error:module-failure:${token}`, "dynamic import exposed its evaluation error");
  output(root, "module-failure", failure);
  await capturePlatformStep(host, capture, "platform-1", "module-import-failed", [
    failure,
    runtime.__scriptModuleOrder.join("|"),
  ]);

  const recovered = (await import(recoveryUrl)) as { recovered: string };
  const final = `${recovered.recovered}|${runtime.__scriptModuleOrder.join("|")}`;
  assertFixture(
    final === `recovered:${token}|throw|recovery`,
    "a separate module graph recovered after a rejected graph",
  );
  output(root, "module-recovery", final);
  await capturePlatformStep(host, capture, "platform-2", "module-import-recovered", [final]);

  delete runtime.__scriptModuleOrder;
  return [fact("failure", failure), fact("recovery", final), fact("order", "throw|recovery")];
}

async function documentOpenRealmReplacement(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = scenarioToken(meta, spec);
  const frame = document.createElement("iframe");
  frame.dataset.scriptFrame = "document-open";
  root.append(frame);
  assertFixture(frame.contentWindow, "document.open frame has a Window");
  const initial = waitForScriptMessage(frame.contentWindow, "realm-initial", "initial document realm");
  frame.srcdoc = `<!doctype html><html><head><meta charset="utf-8"><title>initial realm</title></head><body><main id="old-root"><output>initial:${token}</output></main><script>globalThis.__realmGeneration="initial";parent.postMessage({source:"moli-script-lifecycle",phase:"realm-initial",value:document.title+"|"+globalThis.__realmGeneration},"*");<\/script></body></html>`;
  const first = await initial;
  const oldRoot = frame.contentDocument?.querySelector("#old-root");
  assertFixture(oldRoot?.isConnected, "initial document root is connected");
  output(root, "realm-initial", first.value ?? "missing");
  await capturePlatformStep(host, capture, "platform-1", "document-open-initial", [
    first.value,
    oldRoot.isConnected,
  ]);

  const replacement = waitForScriptMessage(
    frame.contentWindow,
    "realm-replacement",
    "replacement document realm",
  );
  const frameDocument = frame.contentDocument;
  assertFixture(frameDocument, "document.open frame exposes its initial document");
  frameDocument.open();
  frameDocument.write(`<!doctype html><html><head><meta charset="utf-8"><title>replacement realm</title></head><body><main id="new-root"><output>replacement:${token}</output></main><script>globalThis.__realmGeneration="replacement";parent.postMessage({source:"moli-script-lifecycle",phase:"realm-replacement",value:document.title+"|"+globalThis.__realmGeneration},"*");<\/script></body></html>`);
  frameDocument.close();
  const second = await replacement;
  const newDocument = frame.contentDocument;
  assertFixture(newDocument?.querySelector("#new-root"), "replacement document installed its root");
  assertFixture(!oldRoot.isConnected, "document.open disconnected the retired document root");
  output(root, "realm-replacement", `${second.value}|oldConnected=${oldRoot.isConnected}`);
  await capturePlatformStep(host, capture, "platform-2", "document-open-replaced", [
    second.value,
    oldRoot.isConnected,
    newDocument?.title,
  ]);

  return [
    fact("initial", first.value ?? "missing"),
    fact("replacement", second.value ?? "missing"),
    fact("old-connected", oldRoot.isConnected),
  ];
}

async function currentScriptCrossDocumentMove(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  root.dataset.scriptTarget = "";
  const token = scenarioToken(meta, spec);
  const runtime = globalThis as typeof globalThis & { __scriptLifecycleOrder?: string[] };
  runtime.__scriptLifecycleOrder = [];
  const mainScript = document.createElement("script");
  mainScript.dataset.scriptId = "main-script";
  mainScript.src = scriptUrl("/support/scripts/classic.js", { token, label: "main-owner" });
  const mainLoaded = scriptCompletion(mainScript, "main currentScript probe");
  root.append(mainScript);
  await mainLoaded;
  const mainMarker = root.querySelector<HTMLElement>("[data-classic-script='main-owner']");
  const first = `${mainMarker?.dataset.currentScript}|${mainMarker?.dataset.ownerTitle}`;
  assertFixture(first.startsWith("main-script|"), "classic script observed its main currentScript");
  output(root, "current-main", first);
  await capturePlatformStep(host, capture, "platform-1", "current-script-main", [first]);

  const frame = document.createElement("iframe");
  frame.title = "script-owner-frame";
  const loaded = frameLoad(frame, "currentScript owner frame");
  frame.srcdoc = "<!doctype html><html><head><title>frame-owner</title></head><body><main data-script-target></main></body></html>";
  root.append(frame);
  await loaded;
  const frameDocument = frame.contentDocument;
  assertFixture(frameDocument, "currentScript frame exposes its document");
  const frameScript = frameDocument.createElement("script");
  frameScript.dataset.scriptId = "frame-script";
  frameScript.src = scriptUrl("/support/scripts/classic.js", { token, label: "frame-owner" });
  const frameScriptLoaded = scriptCompletion(frameScript, "frame currentScript probe");
  frameDocument.body.append(frameScript);
  await frameScriptLoaded;
  const frameMarker = frameDocument.querySelector<HTMLElement>("[data-classic-script='frame-owner']");
  const beforeMove = `${frameMarker?.dataset.currentScript}|${frameMarker?.dataset.ownerTitle}`;
  document.adoptNode(frameScript);
  root.append(frameScript);
  await microtaskTurns(3);
  const afterMove = runtime.__scriptLifecycleOrder.join("|");
  assertFixture(beforeMove === "frame-script|frame-owner", "frame script observed its source document");
  assertFixture(
    afterMove === "classic:main-owner",
    "moving an evaluated cross-document script did not execute it in the destination",
  );
  output(root, "current-frame", `${beforeMove}|mainOrder=${afterMove}`);
  await capturePlatformStep(host, capture, "platform-2", "current-script-cross-document", [
    beforeMove,
    afterMove,
    frameScript.ownerDocument === document,
  ]);

  delete runtime.__scriptLifecycleOrder;
  return [
    fact("main", first),
    fact("frame", beforeMove),
    fact("destination-order", afterMove),
    fact("adopted", frameScript.ownerDocument === document),
  ];
}

const SCENARIOS: Record<string, ScriptScenario> = {
  "parser-blocking-defer-order": parserBlockingDeferOrder,
  "dynamic-classic-sequential-queue": dynamicClassicSequentialQueue,
  "inert-type-clone-execution": inertTypeCloneExecution,
  "document-write-nested-scripts": documentWriteNestedScripts,
  "module-static-graph-tla": moduleStaticGraphTopLevelAwait,
  "dynamic-import-cache-query": dynamicImportCacheQueryIdentity,
  "import-map-bare-specifier": importMapBareSpecifier,
  "module-failure-recovery": moduleFailureRecovery,
  "document-open-realm-replacement": documentOpenRealmReplacement,
  "current-script-cross-document-move": currentScriptCrossDocumentMove,
};

export async function runScriptModuleLifecycleCase(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformBoundaryResult> {
  const scenario = SCENARIOS[spec.slug];
  assertFixture(scenario, `missing script/module lifecycle scenario ${spec.slug}`);
  const facts = await scenario(host, meta, spec, capture);
  return { status: "ready", facts };
}

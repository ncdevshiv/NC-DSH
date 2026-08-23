import { assertFixture, microtaskTurns } from "./harness";
import type { CaseSpec, SmokeMeta } from "./types";

export interface WebPlatformIntegrationResult {
  status: "ready";
  facts: Array<{
    name: string;
    value: string;
  }>;
}

interface FeedItem {
  id: string;
  label: string;
  state: string;
}

interface IntegrationFeed {
  revision: number;
  channel: string;
  items: FeedItem[];
}

type Fact = WebPlatformIntegrationResult["facts"][number];

function fact(name: string, value: unknown): Fact {
  return { name, value: String(value) };
}

function nodeLabel(node: Node): string {
  if (node.nodeType === Node.TEXT_NODE) {
    return `#text:${node.textContent ?? ""}`;
  }
  if (node instanceof Element) {
    return node.id ? `${node.localName}#${node.id}` : node.localName;
  }
  return node.nodeName;
}

function assignedText(slot: HTMLSlotElement): string {
  return slot
    .assignedNodes({ flatten: true })
    .map((node) => node.textContent?.trim() ?? "")
    .filter(Boolean)
    .join("|");
}

async function appendSrcdocFrame(
  host: HTMLElement,
  title: string,
  srcdoc: string,
): Promise<HTMLIFrameElement> {
  const frame = document.createElement("iframe");
  frame.title = title;
  const loaded = new Promise<void>((resolve, reject) => {
    frame.addEventListener("load", () => resolve(), { once: true });
    frame.addEventListener("error", () => reject(new Error(`${title} failed to load`)), {
      once: true,
    });
  });
  frame.srcdoc = srcdoc;
  host.append(frame);
  await loaded;
  assertFixture(frame.contentDocument, `${title} exposes contentDocument`);
  return frame;
}

async function customElementShadowUpgrade(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
): Promise<Fact[]> {
  const lifecycle: string[] = [];
  const elementName = `x-smoke-${meta.framework}-${spec.seed}`;
  const element = document.createElement(elementName);
  element.id = "upgrade-target";
  element.setAttribute("status", "booting");
  const label = document.createElement("span");
  label.slot = "label";
  label.textContent = `${meta.framework} upgrade`;
  element.append(label);
  host.append(element);

  class SmokeStatusElement extends HTMLElement {
    static get observedAttributes(): string[] {
      return ["status"];
    }

    constructor() {
      super();
      lifecycle.push("constructor");
      const root = this.attachShadow({ mode: "open" });
      const article = document.createElement("article");
      const slot = document.createElement("slot");
      slot.name = "label";
      slot.textContent = "fallback";
      const status = document.createElement("strong");
      status.id = "shadow-status";
      article.append(slot, status);
      root.append(article);
    }

    connectedCallback(): void {
      lifecycle.push("connected");
      this.dataset.connected = "yes";
    }

    attributeChangedCallback(
      _name: string,
      oldValue: string | null,
      newValue: string | null,
    ): void {
      lifecycle.push(`status:${oldValue ?? "null"}>${newValue ?? "null"}`);
      const status = this.shadowRoot?.querySelector("#shadow-status");
      if (status) {
        status.textContent = newValue ?? "missing";
      }
    }
  }

  customElements.define(elementName, SmokeStatusElement);
  await customElements.whenDefined(elementName);
  element.setAttribute("status", "ready");
  await microtaskTurns(2);

  const shadow = element.shadowRoot;
  const slot = shadow?.querySelector("slot");
  assertFixture(shadow, "upgraded custom element has an open shadow root");
  assertFixture(slot instanceof HTMLSlotElement, "upgraded custom element has a label slot");
  assertFixture(element.dataset.connected === "yes", "connectedCallback updated the host");
  assertFixture(
    shadow.querySelector("#shadow-status")?.textContent === "ready",
    "attributeChangedCallback updated shadow content",
  );

  return [
    fact("registry", customElements.get(elementName)?.name ?? "missing"),
    fact("lifecycle", lifecycle.join("|")),
    fact("assigned-label", assignedText(slot)),
    fact("shadow-status", shadow.querySelector("#shadow-status")?.textContent ?? "missing"),
  ];
}

function nestedShadowSlotReassignment(
  host: HTMLElement,
  meta: SmokeMeta,
): Fact[] {
  const outer = document.createElement("section");
  outer.id = "outer-shadow-host";
  const outerRoot = outer.attachShadow({ mode: "open" });
  outerRoot.innerHTML =
    '<article><header><slot name="heading">Heading fallback</slot></header><slot>Body fallback</slot><div id="inner-mount"></div></article>';

  const heading = document.createElement("h2");
  heading.slot = "heading";
  heading.textContent = `${meta.framework} heading old`;
  const body = document.createElement("p");
  body.textContent = "Body projection";
  outer.append(heading, body);
  host.append(outer);

  const inner = document.createElement("div");
  inner.id = "inner-shadow-host";
  const innerRoot = inner.attachShadow({ mode: "open" });
  innerRoot.innerHTML = '<span>Inner:</span><slot name="detail">Detail fallback</slot>';
  const detail = document.createElement("em");
  detail.slot = "detail";
  detail.textContent = "nested detail";
  inner.append(detail);
  outerRoot.querySelector("#inner-mount")?.append(inner);

  const replacement = document.createElement("h3");
  replacement.slot = "heading";
  replacement.textContent = `${meta.framework} heading ready`;
  heading.removeAttribute("slot");
  outer.insertBefore(replacement, heading);

  const headingSlot = outerRoot.querySelector('slot[name="heading"]');
  const bodySlot = outerRoot.querySelector("slot:not([name])");
  const detailSlot = innerRoot.querySelector('slot[name="detail"]');
  assertFixture(headingSlot instanceof HTMLSlotElement, "outer heading slot exists");
  assertFixture(bodySlot instanceof HTMLSlotElement, "outer default slot exists");
  assertFixture(detailSlot instanceof HTMLSlotElement, "inner detail slot exists");
  assertFixture(assignedText(headingSlot).endsWith("heading ready"), "replacement heading assigned");

  return [
    fact("heading-slot", assignedText(headingSlot)),
    fact("body-slot", assignedText(bodySlot)),
    fact("detail-slot", assignedText(detailSlot)),
    fact("shadow-depth", 2),
  ];
}

async function iframeSrcdocAdoption(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
): Promise<Fact[]> {
  const frame = await appendSrcdocFrame(
    host,
    "srcdoc adoption frame",
    `<!doctype html><html><body><main id="child-root"><article id="child-item" data-seed="${spec.seed}"><h2>${meta.framework} child</h2><p>adopt me</p></article><template id="child-template"><aside><strong>template payload</strong></aside></template></main></body></html>`,
  );
  const childDocument = frame.contentDocument;
  const childWindow = frame.contentWindow;
  assertFixture(childDocument, "srcdoc child document is available");
  assertFixture(childWindow, "srcdoc child window is available");
  const childItem = childDocument.querySelector("#child-item");
  const childTemplateNode = childDocument.querySelector("#child-template");
  assertFixture(childItem, "srcdoc child contains adoptable item");
  assertFixture(childTemplateNode?.localName === "template", "srcdoc child contains template");
  const childRealm = childWindow as Window & typeof globalThis;
  assertFixture(
    childTemplateNode instanceof childRealm.HTMLTemplateElement,
    "srcdoc template uses the child realm constructor",
  );
  const childTemplate = childTemplateNode as HTMLTemplateElement;

  const adopted = document.adoptNode(childItem);
  adopted.id = "adopted-item";
  adopted.setAttribute("data-owner", "top-document");
  const imported = document.importNode(childTemplate.content, true);
  const importedContainer = document.createElement("section");
  importedContainer.id = "imported-template";
  importedContainer.append(imported);
  host.append(adopted, importedContainer);

  assertFixture(adopted.ownerDocument === document, "adopted node changed owner document");
  assertFixture(
    childDocument.querySelector("#child-item") === null,
    "adopted node was removed from child document",
  );

  return [
    fact("child-url", childDocument.URL.startsWith("about:srcdoc") ? "about:srcdoc" : childDocument.URL),
    fact("child-elements", childDocument.querySelectorAll("*").length),
    fact("adopted-owner", adopted.ownerDocument === document),
    fact("imported-text", importedContainer.textContent?.trim() ?? ""),
    fact("template-realm", childTemplate instanceof HTMLTemplateElement ? "top" : "child"),
  ];
}

async function iframeDocumentWrite(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
): Promise<Fact[]> {
  const frame = document.createElement("iframe");
  frame.title = "document write frame";
  host.append(frame);
  const childDocument = frame.contentDocument;
  assertFixture(childDocument, "blank iframe exposes contentDocument");
  childDocument.open();
  childDocument.write(`<!doctype html>
<html>
  <head><title>Written ${meta.framework}</title></head>
  <body>
    <main id="written-main" data-seed="${spec.seed}">
      <p id="before-write">before</p>
      <script>document.write("<section id='nested-write'><em>nested payload</em></section>");<\/script>
      <template id="written-template"><ul><li>template one</li><li>template two</li></ul></template>
      <table><tbody><tr><td>parser cell</td></tr></tbody></table>
    </main>
  </body>
</html>`);
  childDocument.close();
  await microtaskTurns(2);

  const nested = childDocument.querySelector("#nested-write");
  const childWindow = frame.contentWindow;
  const templateNode = childDocument.querySelector("#written-template");
  assertFixture(nested?.textContent === "nested payload", "nested document.write executed");
  assertFixture(childWindow, "written frame exposes contentWindow");
  assertFixture(templateNode?.localName === "template", "written template exists");
  const childRealm = childWindow as Window & typeof globalThis;
  assertFixture(
    templateNode instanceof childRealm.HTMLTemplateElement,
    "written template uses the child realm constructor",
  );
  const template = templateNode as HTMLTemplateElement;
  assertFixture(template.content.querySelectorAll("li").length === 2, "template content parsed");

  return [
    fact("doctype", childDocument.doctype?.name ?? "missing"),
    fact("title", childDocument.title),
    fact("written-order", Array.from(childDocument.querySelector("#written-main")?.children ?? []).map((item) => item.id || item.localName).join("|")),
    fact("template-items", template.content.querySelectorAll("li").length),
  ];
}

async function storageCookieRoundtrip(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
): Promise<Fact[]> {
  const prefix = `smoke_${meta.framework}_${spec.seed}`;
  const localKey = `${prefix}_local`;
  const sessionKey = `${prefix}_session`;
  localStorage.setItem(localKey, "booting");
  sessionStorage.setItem(sessionKey, "session-ready");
  document.cookie = `${prefix}=cookie-ready; Path=/; SameSite=Lax`;

  const frame = await appendSrcdocFrame(
    host,
    "storage event frame",
    '<!doctype html><html><body><output id="storage-event">waiting</output></body></html>',
  );
  const childWindow = frame.contentWindow;
  const childDocument = frame.contentDocument;
  assertFixture(childWindow, "storage frame exposes contentWindow");
  assertFixture(childDocument, "storage frame exposes contentDocument");

  const storageEvent = new Promise<string>((resolve) => {
    childWindow.addEventListener(
      "storage",
      (event) => {
        const text = `${event.key}:${event.oldValue}>${event.newValue}`;
        const output = childDocument.querySelector("#storage-event");
        if (output) {
          output.textContent = text;
        }
        resolve(text);
      },
      { once: true },
    );
  });
  localStorage.setItem(localKey, "ready");
  const eventText = await storageEvent;
  const cookies = document.cookie
    .split(";")
    .map((value) => value.trim())
    .filter((value) => value.startsWith(`${prefix}=`))
    .sort()
    .join("|");

  assertFixture(localStorage.getItem(localKey) === "ready", "localStorage roundtrip succeeded");
  assertFixture(
    childWindow.sessionStorage.getItem(sessionKey) === "session-ready",
    "same-origin child sees top-level sessionStorage",
  );
  assertFixture(eventText.endsWith("booting>ready"), "child received storage transition");
  assertFixture(cookies.includes("cookie-ready"), "cookie roundtrip succeeded");

  return [
    fact("local", localStorage.getItem(localKey)),
    fact("session-child", childWindow.sessionStorage.getItem(sessionKey)),
    fact("storage-event", eventText),
    fact("cookie", cookies),
  ];
}

function xhrJson(url: string): Promise<IntegrationFeed> {
  return new Promise((resolve, reject) => {
    const request = new XMLHttpRequest();
    request.open("GET", url);
    request.addEventListener("load", () => {
      if (request.status !== 200) {
        reject(new Error(`XHR returned ${request.status}`));
        return;
      }
      try {
        resolve(JSON.parse(request.responseText) as IntegrationFeed);
      } catch (error) {
        reject(error);
      }
    });
    request.addEventListener("error", () => reject(new Error("XHR network error")));
    request.send();
  });
}

async function fetchXhrDomMerge(
  host: HTMLElement,
  meta: SmokeMeta,
): Promise<Fact[]> {
  const base = "/data/web-platform-feed.json";
  const [fetchResponse, xhrFeed] = await Promise.all([
    fetch(`${base}?transport=fetch&framework=${meta.framework}`),
    xhrJson(`${base}?transport=xhr&framework=${meta.framework}`),
  ]);
  assertFixture(fetchResponse.ok, `fetch returned ${fetchResponse.status}`);
  const fetchFeed = (await fetchResponse.json()) as IntegrationFeed;
  assertFixture(fetchFeed.revision === 7, "fetch returned expected revision");
  assertFixture(xhrFeed.revision === fetchFeed.revision, "XHR and fetch revisions match");
  assertFixture(
    JSON.stringify(xhrFeed.items) === JSON.stringify(fetchFeed.items),
    "XHR and fetch payloads match",
  );

  const table = document.createElement("table");
  table.id = "network-feed";
  const body = document.createElement("tbody");
  for (const item of fetchFeed.items) {
    const row = document.createElement("tr");
    for (const value of [item.id, item.label, item.state]) {
      const cell = document.createElement("td");
      cell.textContent = value;
      row.append(cell);
    }
    body.append(row);
  }
  table.append(body);
  host.append(table);

  return [
    fact("revision", fetchFeed.revision),
    fact("channel", xhrFeed.channel),
    fact("rows", body.rows.length),
    fact("states", fetchFeed.items.map((item) => item.state).join("|")),
  ];
}

async function mutationObserverFragmentBatch(host: HTMLElement): Promise<Fact[]> {
  const records: string[] = [];
  const observer = new MutationObserver((batch) => {
    for (const record of batch) {
      records.push(
        [
          record.type,
          nodeLabel(record.target),
          record.attributeName ?? "-",
          record.oldValue ?? "-",
          Array.from(record.addedNodes).map(nodeLabel).join(",") || "-",
          Array.from(record.removedNodes).map(nodeLabel).join(",") || "-",
        ].join(":"),
      );
    }
  });
  observer.observe(host, {
    subtree: true,
    childList: true,
    attributes: true,
    attributeOldValue: true,
    characterData: true,
    characterDataOldValue: true,
  });

  const fragment = document.createDocumentFragment();
  const first = document.createElement("p");
  first.id = "observer-first";
  first.dataset.phase = "one";
  first.textContent = "alpha";
  const second = document.createElement("p");
  second.id = "observer-second";
  second.textContent = "beta";
  fragment.append(first, second);
  host.append(fragment);
  first.dataset.phase = "two";
  const firstText = first.firstChild;
  assertFixture(firstText instanceof Text, "observer target has a text child");
  firstText.data = "alpha-ready";
  host.insertBefore(second, first);
  await microtaskTurns(2);
  observer.disconnect();

  const list = document.createElement("ol");
  list.id = "mutation-records";
  records.forEach((record) => {
    const item = document.createElement("li");
    item.textContent = record;
    list.append(item);
  });
  host.append(list);

  assertFixture(records.some((record) => record.startsWith("attributes:")), "attribute record exists");
  assertFixture(
    records.some((record) => record.startsWith("characterData:")),
    "characterData record exists",
  );
  assertFixture(records.filter((record) => record.startsWith("childList:")).length >= 3, "childList move records exist");

  return [
    fact("records", records.length),
    fact("types", records.map((record) => record.split(":", 1)[0]).join("|")),
    fact("final-order", Array.from(host.children).map((element) => element.id).join("|")),
  ];
}

function rangeTreeWalkerRewrite(host: HTMLElement): Fact[] {
  const article = document.createElement("article");
  article.id = "range-article";
  article.innerHTML =
    '<p id="range-line"><span>alpha brave</span><strong> charlie delta</strong><em> echo</em></p>';
  host.append(article);
  const spanText = article.querySelector("span")?.firstChild;
  const strongText = article.querySelector("strong")?.firstChild;
  assertFixture(spanText instanceof Text, "range start text exists");
  assertFixture(strongText instanceof Text, "range end text exists");

  const range = document.createRange();
  range.setStart(spanText, 6);
  range.setEnd(strongText, 8);
  const extracted = range.extractContents();
  const extractedText = extracted.textContent ?? "";
  const mark = document.createElement("mark");
  mark.id = "range-extract";
  mark.append(extracted);
  range.insertNode(mark);

  const traversal: string[] = [];
  const walker = document.createTreeWalker(
    article,
    NodeFilter.SHOW_ELEMENT | NodeFilter.SHOW_TEXT,
    {
      acceptNode(node) {
        if (node.nodeType === Node.TEXT_NODE && !(node.textContent ?? "").trim()) {
          return NodeFilter.FILTER_SKIP;
        }
        return NodeFilter.FILTER_ACCEPT;
      },
    },
  );
  while (walker.nextNode()) {
    traversal.push(nodeLabel(walker.currentNode));
  }

  assertFixture(extractedText === "brave charlie", "Range extracted cross-element text");
  assertFixture(mark.textContent === extractedText, "Range insertion preserved extracted text");

  return [
    fact("extracted", extractedText),
    fact("mark-parent", mark.parentElement?.localName ?? "missing"),
    fact("traversal", traversal.join("|")),
    fact("article-text", article.textContent),
  ];
}

function formDataSearchParamsPipeline(host: HTMLElement): Fact[] {
  const form = document.createElement("form");
  form.id = "platform-form";
  form.innerHTML = `
    <input name="alpha" value="one">
    <input name="flag" type="checkbox" value="yes" checked>
    <input name="unchecked" type="checkbox" value="no">
    <select name="choice" multiple>
      <option value="a" selected>Alpha</option>
      <option value="b" selected>Beta</option>
      <option value="c">Gamma</option>
    </select>
    <textarea name="notes">line one
line two</textarea>
    <fieldset disabled><input name="ignored" value="disabled"></fieldset>
  `;
  host.append(form);

  const data = new FormData(form);
  data.append("alpha", "two");
  data.set("mode", "ready");
  const entries = Array.from(data.entries()).map(
    ([name, value]) => [name, String(value)] as [string, string],
  );
  const params = new URLSearchParams(entries);
  params.sort();

  const list = document.createElement("ol");
  list.id = "formdata-entries";
  entries.forEach(([name, value]) => {
    const item = document.createElement("li");
    item.dataset.name = name;
    item.textContent = `${name}=${value}`;
    list.append(item);
  });
  const output = document.createElement("output");
  output.id = "search-params";
  output.textContent = params.toString();
  host.append(list, output);

  assertFixture(!data.has("ignored"), "disabled fieldset entry is excluded");
  assertFixture(data.getAll("choice").join("|") === "a|b", "multiple select entries preserved");
  assertFixture(data.getAll("alpha").join("|") === "one|two", "duplicate form entries preserved");

  return [
    fact("entries", entries.map(([name, value]) => `${name}=${value}`).join("|")),
    fact("choices", data.getAll("choice").join("|")),
    fact("params", params.toString()),
    fact("ignored", data.has("ignored")),
  ];
}

function shadowEventAbortPipeline(host: HTMLElement): Fact[] {
  const outer = document.createElement("div");
  outer.id = "event-shadow-host";
  const root = outer.attachShadow({ mode: "open" });
  const button = document.createElement("button");
  button.id = "event-button";
  button.textContent = "Dispatch";
  root.append(button);
  host.append(outer);

  const controller = new AbortController();
  const log: string[] = [];
  const targetName = (target: EventTarget | null): string => {
    if (target === outer) return "outer";
    if (target === button) return "button";
    return target instanceof Node ? target.nodeName : "unknown";
  };
  const listener = (name: string) => (event: Event) => {
    log.push(`${name}:${targetName(event.target)}:${event.eventPhase}`);
  };

  outer.addEventListener("smoke-pulse", listener("outer-capture"), { capture: true });
  root.addEventListener("smoke-pulse", listener("shadow-capture"), { capture: true });
  button.addEventListener("smoke-pulse", listener("button-once"), {
    once: true,
    signal: controller.signal,
  });
  root.addEventListener("smoke-pulse", listener("shadow-signal"), {
    signal: controller.signal,
  });
  outer.addEventListener("smoke-pulse", listener("outer-bubble"));

  button.dispatchEvent(
    new CustomEvent("smoke-pulse", { bubbles: true, composed: true, detail: "first" }),
  );
  controller.abort();
  button.dispatchEvent(
    new CustomEvent("smoke-pulse", { bubbles: true, composed: true, detail: "second" }),
  );

  const list = document.createElement("ol");
  list.id = "event-log";
  log.forEach((entry) => {
    const item = document.createElement("li");
    item.textContent = entry;
    list.append(item);
  });
  host.append(list);

  assertFixture(
    log.filter((entry) => entry.startsWith("button-once:")).length === 1,
    "once listener ran exactly once",
  );
  assertFixture(
    log.filter((entry) => entry.startsWith("shadow-signal:")).length === 1,
    "aborted shadow listener did not run twice",
  );
  assertFixture(
    log.filter((entry) => entry.startsWith("outer-bubble:")).length === 2,
    "persistent outer listener ran twice",
  );

  return [
    fact("events", log.length),
    fact("log", log.join("|")),
    fact("aborted", controller.signal.aborted),
    fact("shadow-mode", root.mode),
  ];
}

export async function runWebPlatformIntegrationCase(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
): Promise<WebPlatformIntegrationResult> {
  host.replaceChildren();
  host.dataset.integrationCase = spec.slug;
  let facts: Fact[];
  switch (spec.slug) {
    case "custom-element-shadow-upgrade":
      facts = await customElementShadowUpgrade(host, meta, spec);
      break;
    case "nested-shadow-slot-reassignment":
      facts = nestedShadowSlotReassignment(host, meta);
      break;
    case "iframe-srcdoc-adoption":
      facts = await iframeSrcdocAdoption(host, meta, spec);
      break;
    case "iframe-document-write":
      facts = await iframeDocumentWrite(host, meta, spec);
      break;
    case "storage-cookie-roundtrip":
      facts = await storageCookieRoundtrip(host, meta, spec);
      break;
    case "fetch-xhr-dom-merge":
      facts = await fetchXhrDomMerge(host, meta);
      break;
    case "mutation-observer-fragment-batch":
      facts = await mutationObserverFragmentBatch(host);
      break;
    case "range-treewalker-rewrite":
      facts = rangeTreeWalkerRewrite(host);
      break;
    case "formdata-searchparams-pipeline":
      facts = formDataSearchParamsPipeline(host);
      break;
    case "shadow-event-abort-pipeline":
      facts = shadowEventAbortPipeline(host);
      break;
    default:
      throw new Error(`unknown web-platform integration case: ${spec.slug}`);
  }
  host.dataset.integrationComplete = "true";
  return { status: "ready", facts };
}

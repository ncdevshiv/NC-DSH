import { assertFixture } from "./harness";
import {
  capturePlatformStep,
  fact,
  type CapturePlatformFrame,
  type PlatformBoundaryResult,
  type PlatformFact,
} from "./platform-boundary";
import type { CaseSpec, SmokeMeta } from "./types";

type ParserScenario = (
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
) => Promise<PlatformFact[]>;

function scenarioRoot(host: HTMLElement, name: string): HTMLElement {
  const root = document.createElement("section");
  root.dataset.parserScenario = name;
  host.append(root);
  return root;
}

function elementChildren(node: ParentNode): Element[] {
  return Array.from(node.children);
}

async function domParserHtmlTemplate(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const parsed = new DOMParser().parseFromString(
    `<!doctype html><html lang="en"><head><title>Detached ${spec.seed}</title></head><body>
      <template id="payload"><article data-seed="${spec.seed}"><h2>${meta.framework}</h2>
      <table><tbody><tr><td>alpha</td><td>beta</td></tr></tbody></table></article></template>
      <p id="outside">outside-${spec.variant}</p></body></html>`,
    "text/html",
  );
  const template = parsed.querySelector("#payload");
  assertFixture(template instanceof HTMLTemplateElement, "parsed HTML exposes template content");
  assertFixture(parsed.doctype?.name === "html", "parsed HTML retains its doctype");

  const firstClone = document.importNode(template.content, true);
  root.append(firstClone);
  const liveArticle = root.querySelector("article");
  assertFixture(liveArticle instanceof HTMLElement, "template clone entered the live document");
  await capturePlatformStep(host, capture, "platform-1", "html-template-import", [
    parsed.title,
    liveArticle.dataset.seed ?? "missing",
    root.querySelectorAll("td").length,
  ]);

  liveArticle.dataset.phase = "updated";
  const heading = liveArticle.querySelector("h2");
  assertFixture(heading, "template clone retained its heading");
  heading.textContent = `${heading.textContent}:${spec.variant}`;
  const outside = parsed.querySelector("#outside");
  assertFixture(outside, "parsed HTML retained its outside paragraph");
  root.append(document.importNode(outside, true));
  await capturePlatformStep(host, capture, "platform-2", "html-template-update", [
    liveArticle.dataset.phase ?? "missing",
    heading.textContent ?? "",
    root.children.length,
  ]);

  return [
    fact("content-type", parsed.contentType),
    fact("doctype", parsed.doctype?.name ?? "missing"),
    fact("template-elements", template.content.querySelectorAll("*").length),
    fact("live-owner", liveArticle.ownerDocument === document),
    fact("live-text", root.textContent?.replaceAll(/\s+/g, " ").trim() ?? ""),
  ];
}

async function domParserXmlErrors(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const parser = new DOMParser();
  const parsed = parser.parseFromString(
    `<catalog xmlns="urn:catalog" xmlns:m="urn:meta"><item id="i-${spec.seed}"><![CDATA[alpha<beta]]></item><m:meta m:kind="stable"/></catalog>`,
    "application/xml",
  );
  const catalog = parsed.documentElement;
  assertFixture(catalog.localName === "catalog", "valid XML parsed its document element");
  assertFixture(catalog.namespaceURI === "urn:catalog", "valid XML retained default namespace");
  const imported = document.importNode(catalog, true);
  root.append(imported);
  await capturePlatformStep(host, capture, "platform-1", "xml-import", [
    imported.localName,
    imported.namespaceURI ?? "null",
    imported.childNodes.length,
  ]);

  const invalid = parser.parseFromString("<catalog><item></catalog>", "application/xml");
  const parserErrors = invalid.getElementsByTagName("parsererror");
  assertFixture(parserErrors.length === 1, "invalid XML materialized one parsererror element");
  const summary = document.createElement("output");
  summary.dataset.xmlErrorCount = String(parserErrors.length);
  summary.textContent = `${invalid.documentElement.localName}:${parserErrors.length}`;
  root.append(summary);
  await capturePlatformStep(host, capture, "platform-2", "xml-error", [
    invalid.documentElement.localName,
    parserErrors.length,
    invalid.contentType,
  ]);

  const item = imported.getElementsByTagNameNS("urn:catalog", "item")[0];
  const metadata = imported.getElementsByTagNameNS("urn:meta", "meta")[0];
  assertFixture(item && metadata, "imported XML retained namespaced descendants");
  return [
    fact("root-namespace", imported.namespaceURI ?? "null"),
    fact("item-text", item.textContent ?? ""),
    fact("item-first-node", item.firstChild?.nodeName ?? "missing"),
    fact("metadata-kind", metadata.getAttributeNS("urn:meta", "kind") ?? "missing"),
    fact("parser-errors", parserErrors.length),
  ];
}

async function contextualFragmentTable(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const table = document.createElement("table");
  table.innerHTML = `<caption>batch-${spec.seed}</caption><tbody></tbody>`;
  root.append(table);
  const body = table.tBodies[0];
  assertFixture(body, "table parser created a tbody");

  const bodyRange = document.createRange();
  bodyRange.selectNodeContents(body);
  const rows = bodyRange.createContextualFragment(
    `<tr data-row="first"><td>A${spec.variant}</td><td><template><em>hidden-a</em></template>B</td></tr>
     <tr data-row="second"><td colspan="2">C${spec.seed}</td></tr>`,
  );
  body.append(rows);
  assertFixture(body.rows.length === 2, "tbody contextual fragment produced two rows");
  await capturePlatformStep(host, capture, "platform-1", "table-context", [
    body.rows.length,
    body.rows[0]?.cells.length ?? 0,
    body.rows[1]?.cells.length ?? 0,
  ]);

  body.rows[0]?.insertAdjacentHTML(
    "beforeend",
    `<td data-extra="${spec.seed}"><strong>extra</strong></td>`,
  );
  body.insertAdjacentHTML(
    "afterbegin",
    `<tr data-row="prepended"><td>P</td><td>${spec.variant}</td></tr>`,
  );
  await capturePlatformStep(host, capture, "platform-2", "table-insert", [
    Array.from(body.rows, (row) => row.dataset.row).join(","),
    body.rows[1]?.cells.length ?? 0,
  ]);

  const nestedTemplate = body.querySelector("template");
  assertFixture(
    nestedTemplate instanceof HTMLTemplateElement,
    "table fragment retained nested template content",
  );
  return [
    fact("row-order", Array.from(body.rows, (row) => row.dataset.row).join("|")),
    fact("cell-counts", Array.from(body.rows, (row) => row.cells.length).join("|")),
    fact("template-text", nestedTemplate.content.textContent ?? ""),
    fact("caption", table.caption?.textContent ?? "missing"),
  ];
}

async function documentFragmentAdoptImport(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const detached = document.implementation.createHTMLDocument(`Source ${spec.seed}`);
  detached.body.innerHTML = `<article id="adopt-me"><h2>${meta.framework}</h2><p>source</p></article>
    <template id="import-me"><ul><li>one</li><li>two</li></ul></template>`;
  const sourceArticle = detached.querySelector("#adopt-me");
  assertFixture(sourceArticle instanceof HTMLElement, "detached document has adoptable article");
  const ownerBefore = sourceArticle.ownerDocument === detached;
  const adopted = document.adoptNode(sourceArticle);
  root.append(adopted);
  assertFixture(adopted.ownerDocument === document, "adopted node changed ownerDocument");
  await capturePlatformStep(host, capture, "platform-1", "document-adopt", [
    ownerBefore,
    detached.querySelector("#adopt-me") === null,
    adopted.ownerDocument === document,
  ]);

  const sourceTemplate = detached.querySelector("#import-me");
  assertFixture(
    sourceTemplate instanceof HTMLTemplateElement,
    "detached document has importable template",
  );
  const imported = document.importNode(sourceTemplate.content, true);
  const importedList = imported.querySelector("ul");
  assertFixture(importedList, "importNode deeply cloned template descendants");
  importedList.dataset.imported = String(spec.variant);
  root.append(imported);
  await capturePlatformStep(host, capture, "platform-2", "document-import", [
    root.querySelectorAll("li").length,
    sourceTemplate.content.querySelectorAll("li").length,
    importedList.ownerDocument === document,
  ]);

  return [
    fact("adopt-owner-before", ownerBefore),
    fact("adopt-source-removed", detached.querySelector("#adopt-me") === null),
    fact("import-count", root.querySelectorAll("li").length),
    fact("source-count", sourceTemplate.content.querySelectorAll("li").length),
    fact("root-text", root.textContent?.replaceAll(/\s+/g, " ").trim() ?? ""),
  ];
}

async function rangeExtractSurround(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const paragraph = document.createElement("p");
  const left = document.createTextNode("alpha ");
  const emphasis = document.createElement("em");
  emphasis.textContent = `bravo-${spec.variant}`;
  const right = document.createTextNode(" charlie");
  paragraph.append(left, emphasis, right);
  root.append(paragraph);

  const range = document.createRange();
  range.setStart(left, 2);
  range.setEnd(right, 4);
  const cloned = range.cloneContents();
  const extracted = range.extractContents();
  const cloneOutput = document.createElement("output");
  cloneOutput.dataset.rangeClone = "";
  cloneOutput.append(cloned);
  const extractOutput = document.createElement("output");
  extractOutput.dataset.rangeExtract = "";
  extractOutput.append(extracted);
  root.append(cloneOutput, extractOutput);
  await capturePlatformStep(host, capture, "platform-1", "range-extract", [
    paragraph.textContent ?? "",
    cloneOutput.textContent ?? "",
    extractOutput.textContent ?? "",
  ]);

  const target = document.createElement("p");
  target.textContent = `delta echo ${spec.seed}`;
  root.append(target);
  const targetText = target.firstChild;
  assertFixture(targetText instanceof Text, "surround target exposes a text node");
  const surround = document.createRange();
  surround.setStart(targetText, 6);
  surround.setEnd(targetText, 10);
  const mark = document.createElement("mark");
  mark.dataset.surrounded = "";
  surround.surroundContents(mark);
  await capturePlatformStep(host, capture, "platform-2", "range-surround", [
    mark.textContent ?? "",
    target.childNodes.length,
    target.textContent ?? "",
  ]);

  return [
    fact("collapsed-after-extract", range.collapsed),
    fact("remaining", paragraph.textContent ?? ""),
    fact("clone", cloneOutput.textContent ?? ""),
    fact("extract", extractOutput.textContent ?? ""),
    fact("surrounded", target.innerHTML),
  ];
}

async function iteratorMutationFilter(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const tree = document.createElement("div");
  tree.innerHTML = `<section data-visit="alpha"><span data-visit="alpha-child">A</span></section>
    <section data-skip=""><span data-visit="beta-child">B</span></section>
    <section data-visit="gamma"><span>G</span></section>`;
  root.append(tree);
  const iterator = document.createNodeIterator(tree, NodeFilter.SHOW_ELEMENT, {
    acceptNode(node) {
      return node instanceof Element && node.hasAttribute("data-visit")
        ? NodeFilter.FILTER_ACCEPT
        : NodeFilter.FILTER_SKIP;
    },
  });
  const first = iterator.nextNode();
  assertFixture(first instanceof Element, "NodeIterator returned its first accepted element");
  const firstName = first.getAttribute("data-visit") ?? "missing";
  first.remove();
  const inserted = document.createElement("section");
  inserted.dataset.visit = `inserted-${spec.variant}`;
  inserted.textContent = "I";
  tree.insertBefore(inserted, tree.children[1] ?? null);
  await capturePlatformStep(host, capture, "platform-1", "iterator-mutation", [
    firstName,
    elementChildren(tree).map((item) => item.getAttribute("data-visit") ?? "skip").join(","),
  ]);

  const remaining: string[] = [];
  for (let node = iterator.nextNode(); node; node = iterator.nextNode()) {
    assertFixture(node instanceof Element, "filtered iterator only returned elements");
    remaining.push(node.getAttribute("data-visit") ?? "missing");
  }
  const walker = document.createTreeWalker(tree, NodeFilter.SHOW_ELEMENT, {
    acceptNode(node) {
      return node instanceof Element && node.hasAttribute("data-skip")
        ? NodeFilter.FILTER_REJECT
        : node instanceof Element && node.hasAttribute("data-visit")
          ? NodeFilter.FILTER_ACCEPT
          : NodeFilter.FILTER_SKIP;
    },
  });
  const walked: string[] = [];
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    assertFixture(node instanceof Element, "TreeWalker returned an element");
    walked.push(node.getAttribute("data-visit") ?? "missing");
  }
  const output = document.createElement("output");
  output.textContent = `${remaining.join(",")};${walked.join(",")}`;
  root.append(output);
  await capturePlatformStep(host, capture, "platform-2", "walker-result", [
    remaining.join(","),
    walked.join(","),
  ]);

  return [
    fact("iterator-first", firstName),
    fact("iterator-rest", remaining.join("|")),
    fact("walker", walked.join("|")),
    fact("tree-text", tree.textContent?.replaceAll(/\s+/g, "").trim() ?? ""),
  ];
}

async function textSplitNormalize(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const paragraph = document.createElement("p");
  const first = document.createTextNode("alpha-beta-gamma");
  paragraph.append(first);
  root.append(paragraph);
  const second = first.splitText(5);
  second.replaceData(1, 4, `B${spec.variant}`);
  paragraph.insertBefore(document.createTextNode(""), second);
  paragraph.append(document.createTextNode("+"), document.createTextNode(String(spec.seed)));
  const beforeWholeText = first.wholeText;
  await capturePlatformStep(host, capture, "platform-1", "text-split", [
    paragraph.childNodes.length,
    first.data,
    second.data,
    beforeWholeText,
  ]);

  first.appendData("/");
  second.insertData(second.length, "/tail");
  paragraph.normalize();
  const normalized = paragraph.firstChild;
  assertFixture(normalized instanceof Text, "normalize retained one text node");
  assertFixture(paragraph.childNodes.length === 1, "normalize merged adjacent text nodes");
  await capturePlatformStep(host, capture, "platform-2", "text-normalize", [
    paragraph.childNodes.length,
    normalized.data,
    normalized.wholeText,
  ]);

  return [
    fact("before-whole-text", beforeWholeText),
    fact("after-child-count", paragraph.childNodes.length),
    fact("after-data", normalized.data),
    fact("after-length", normalized.length),
  ];
}

async function xmlSerializerNamespaces(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const xml = document.implementation.createDocument("urn:catalog", "c:catalog", null);
  const catalog = xml.documentElement;
  catalog.setAttributeNS("http://www.w3.org/2000/xmlns/", "xmlns:m", "urn:meta");
  const item = xml.createElementNS("urn:catalog", "c:item");
  item.setAttributeNS("urn:meta", "m:code", `code-${spec.seed}`);
  item.append(xml.createTextNode("alpha & beta"));
  catalog.append(item);
  const serializer = new XMLSerializer();
  const firstSerialization = serializer.serializeToString(xml);
  const reparsed = new DOMParser().parseFromString(firstSerialization, "application/xml");
  assertFixture(
    reparsed.documentElement.namespaceURI === "urn:catalog",
    "serialized XML reparsed with the catalog namespace",
  );
  const imported = document.importNode(reparsed.documentElement, true);
  root.append(imported);
  await capturePlatformStep(host, capture, "platform-1", "xml-roundtrip", [
    imported.prefix ?? "null",
    imported.namespaceURI ?? "null",
    imported.children.length,
  ]);

  const metadata = document.createElementNS("urn:meta", "m:status");
  metadata.setAttribute("state", "ready");
  metadata.textContent = String(spec.variant);
  imported.append(metadata);
  const secondSerialization = serializer.serializeToString(imported);
  const output = document.createElement("output");
  output.dataset.serialization = "updated";
  output.textContent = secondSerialization;
  root.append(output);
  await capturePlatformStep(host, capture, "platform-2", "xml-update", [
    metadata.prefix ?? "null",
    metadata.namespaceURI ?? "null",
    secondSerialization.length,
  ]);

  return [
    fact("initial-serialization", firstSerialization),
    fact("updated-serialization", secondSerialization),
    fact("item-code", item.getAttributeNS("urn:meta", "code") ?? "missing"),
    fact("status-namespace", metadata.namespaceURI ?? "null"),
  ];
}

async function insertAdjacentParser(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const wrapper = document.createElement("article");
  wrapper.innerHTML = `<div data-anchor=""><span data-center="">center</span></div>`;
  root.append(wrapper);
  const anchor = wrapper.querySelector("[data-anchor]");
  assertFixture(anchor instanceof HTMLElement, "insertAdjacentHTML anchor exists");
  anchor.insertAdjacentHTML("beforebegin", `<p data-position="before">B${spec.variant}</p>`);
  anchor.insertAdjacentHTML(
    "afterbegin",
    `<template data-position="template"><strong>inert-${spec.seed}</strong></template><i data-position="first">F</i>`,
  );
  await capturePlatformStep(host, capture, "platform-1", "adjacent-first", [
    elementChildren(wrapper).map((item) => item.getAttribute("data-position") ?? "anchor").join(","),
    elementChildren(anchor).map((item) => item.getAttribute("data-position") ?? "center").join(","),
  ]);

  anchor.insertAdjacentHTML("beforeend", `<b data-position="last">L</b>`);
  anchor.insertAdjacentHTML("afterend", `<p data-position="after">A${spec.seed}</p>`);
  await capturePlatformStep(host, capture, "platform-2", "adjacent-second", [
    elementChildren(wrapper).map((item) => item.getAttribute("data-position") ?? "anchor").join(","),
    elementChildren(anchor).map((item) => item.getAttribute("data-position") ?? "center").join(","),
  ]);

  const template = anchor.querySelector("template");
  assertFixture(template instanceof HTMLTemplateElement, "adjacent parser retained template");
  return [
    fact(
      "outer-order",
      elementChildren(wrapper).map((item) => item.getAttribute("data-position") ?? "anchor").join("|"),
    ),
    fact(
      "inner-order",
      elementChildren(anchor).map((item) => item.getAttribute("data-position") ?? "center").join("|"),
    ),
    fact("template-text", template.content.textContent ?? ""),
    fact("wrapper-text", wrapper.textContent?.replaceAll(/\s+/g, " ").trim() ?? ""),
  ];
}

async function createHtmlDocumentBase(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const detached = document.implementation.createHTMLDocument(`Detached ${spec.seed}`);
  const base = detached.createElement("base");
  base.href = `${location.origin}/support/base/`;
  detached.head.prepend(base);
  const anchor = detached.createElement("a");
  anchor.href = `../item-${spec.variant}?framework=${meta.framework}#result`;
  anchor.textContent = "detached link";
  const article = detached.createElement("article");
  article.dataset.owner = meta.framework;
  article.append(anchor);
  const template = detached.createElement("template");
  template.innerHTML = `<footer data-seed="${spec.seed}">detached footer</footer>`;
  detached.body.append(article, template);
  const resolved = new URL(anchor.href);
  assertFixture(
    resolved.pathname === `/support/item-${spec.variant}`,
    "detached document resolved its relative URL through base",
  );
  root.append(document.importNode(article, true));
  await capturePlatformStep(host, capture, "platform-1", "detached-document", [
    detached.title,
    resolved.pathname,
    detached.body.children.length,
  ]);

  detached.title = `Updated ${meta.framework}`;
  const importedTemplate = document.importNode(template.content, true);
  root.append(importedTemplate);
  const titleOutput = document.createElement("output");
  titleOutput.dataset.detachedTitle = "";
  titleOutput.textContent = detached.title;
  root.append(titleOutput);
  await capturePlatformStep(host, capture, "platform-2", "detached-update", [
    detached.title,
    root.querySelectorAll("footer").length,
    root.querySelector("a")?.ownerDocument === document,
  ]);

  return [
    fact("detached-content-type", detached.contentType),
    fact("resolved-path", resolved.pathname),
    fact("resolved-query", resolved.search),
    fact("import-owner", root.querySelector("a")?.ownerDocument === document),
    fact("detached-title", detached.title),
  ];
}

const SCENARIOS: Readonly<Record<string, ParserScenario>> = Object.freeze({
  "domparser-html-template": domParserHtmlTemplate,
  "domparser-xml-errors": domParserXmlErrors,
  "contextual-fragment-table": contextualFragmentTable,
  "document-fragment-adopt-import": documentFragmentAdoptImport,
  "range-extract-surround": rangeExtractSurround,
  "iterator-mutation-filter": iteratorMutationFilter,
  "text-split-normalize": textSplitNormalize,
  "xmlserializer-namespaces": xmlSerializerNamespaces,
  "insert-adjacent-parser": insertAdjacentParser,
  "create-html-document-base": createHtmlDocumentBase,
});

export async function runParserDocumentBoundaryCase(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformBoundaryResult> {
  const scenario = SCENARIOS[spec.slug];
  assertFixture(scenario, `unknown parser/document scenario ${spec.slug}`);
  const facts = await scenario(host, meta, spec, capture);
  assertFixture(facts.length >= 4, `${spec.slug} exposes at least four parser facts`);
  return { status: "ready", facts };
}

import { assertFixture } from "./harness";
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

type BinaryScenario = (
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
) => Promise<PlatformFact[]>;

function scenarioRoot(host: HTMLElement, name: string): HTMLElement {
  const root = document.createElement("section");
  root.dataset.binaryScenario = name;
  host.append(root);
  return root;
}

function output(root: HTMLElement, name: string, value: string): HTMLOutputElement {
  const element = document.createElement("output");
  element.dataset.binaryOutput = name;
  element.textContent = value;
  root.append(element);
  return element;
}

function bytes(value: ArrayBuffer | ArrayBufferView): number[] {
  if (value instanceof ArrayBuffer) {
    return Array.from(new Uint8Array(value));
  }
  return Array.from(
    new Uint8Array(value.buffer, value.byteOffset, value.byteLength),
  );
}

async function urlResolutionMutation(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const url = new URL(
    `../assets/../reports/${meta.framework}?q=hello%20world&seed=${spec.seed}#draft`,
    location.href,
  );
  assertFixture(url.origin === location.origin, "relative URL retained the fixture origin");
  assertFixture(url.searchParams.get("q") === "hello world", "URL decoded query text");
  output(
    root,
    "initial-url",
    `${url.pathname}\n${url.search}\n${url.hash}\n${url.searchParams.get("q")}`,
  );
  await capturePlatformStep(host, capture, "platform-1", "url-resolved", [
    url.pathname,
    url.searchParams.size,
    url.hash,
  ]);

  url.pathname = `/binary/${meta.framework}/../case-${spec.variant}`;
  url.searchParams.set("q", `updated ${spec.variant}`);
  url.searchParams.append("tag", "café");
  url.hash = `#done-${spec.seed}`;
  assertFixture(url.pathname === `/binary/case-${spec.variant}`, "URL normalized dot segments");
  assertFixture(url.href.startsWith(location.origin), "mutated URL retained its origin");
  output(
    root,
    "mutated-url",
    `${url.pathname}\n${url.search}\n${url.hash}\n${url.searchParams.getAll("tag").join("|")}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "url-mutated", [
    url.pathname,
    url.searchParams.toString(),
    url.hash,
  ]);

  return [
    fact("path", url.pathname),
    fact("query", url.searchParams.toString()),
    fact("hash", url.hash),
    fact("same-origin", url.origin === location.origin),
  ];
}

async function searchParamsLiveBinding(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const url = new URL(
    `https://example.test/catalog?a=1&a=2&space=hello%20world&owner=${meta.framework}`,
  );
  const params = url.searchParams;
  const initialEntries = Array.from(params, ([key, value]) => `${key}=${value}`);
  assertFixture(params.getAll("a").join("|") === "1|2", "duplicate params retained order");
  output(root, "params-initial", `${initialEntries.join("\n")}\nsize=${params.size}`);
  await capturePlatformStep(host, capture, "platform-1", "params-live-initial", [
    params.size,
    params.getAll("a").join(","),
    url.search,
  ]);

  params.append("a", String(spec.variant));
  params.set("space", `next value ${spec.seed}`);
  params.delete("owner");
  params.sort();
  assertFixture(url.search === `?${params.toString()}`, "URL reflected live params mutation");
  url.search = `?z=last&z=first&framework=${meta.framework}`;
  assertFixture(params.getAll("z").join("|") === "last|first", "params reflected URL.search replacement");
  params.sort();
  const finalEntries = Array.from(params.entries(), ([key, value]) => `${key}=${value}`);
  output(root, "params-final", `${finalEntries.join("\n")}\nsize=${params.size}`);
  await capturePlatformStep(host, capture, "platform-2", "params-live-final", [
    params.size,
    params.toString(),
    url.search,
  ]);

  return [
    fact("initial", initialEntries.join("|")),
    fact("final", finalEntries.join("|")),
    fact("live", url.search === `?${params.toString()}`),
    fact("size", params.size),
  ];
}

async function searchParamsIteratorMutation(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const params = new URLSearchParams(`a=1&b=2&c=3&a=${spec.variant}`);
  const keysBefore = Array.from(params.keys());
  output(root, "iterator-before", `${keysBefore.join("|")}\n${params.getAll("a").join("|")}`);
  await capturePlatformStep(host, capture, "platform-1", "params-iterator-before", [
    keysBefore.join(","),
    params.size,
  ]);

  const visited: string[] = [];
  params.forEach((value, key) => {
    visited.push(`${key}:${value}`);
    if (key === "a" && value === "1") {
      params.append("d", String(spec.seed));
    }
    if (key === "b") {
      params.delete("c");
    }
  });
  const valuesAfter = Array.from(params.values());
  assertFixture(visited.includes(`d:${spec.seed}`), "live forEach visited appended entry");
  assertFixture(!visited.includes("c:3"), "live forEach skipped deleted entry");
  output(
    root,
    "iterator-after",
    `${visited.join("|")}\n${Array.from(params).map((entry) => entry.join("=")).join("|")}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "params-iterator-after", [
    visited.join(","),
    valuesAfter.join(","),
    params.size,
  ]);

  return [
    fact("keys-before", keysBefore.join("|")),
    fact("visited", visited.join("|")),
    fact("values-after", valuesAfter.join("|")),
    fact("serialized", params.toString()),
  ];
}

async function blobUnicodeSlice(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const prefix = new Uint8Array([0, spec.variant, 255]);
  const blob = new Blob([prefix, ":", meta.framework, ":café:東京"], {
    type: "Text/Plain;Charset=UTF-8",
  });
  assertFixture(blob.type === "text/plain;charset=utf-8", "Blob normalized its MIME type");
  output(root, "blob-metadata", `${blob.size}\n${blob.type}\n${prefix.join("|")}`);
  await capturePlatformStep(host, capture, "platform-1", "blob-created", [
    blob.size,
    blob.type,
    prefix.byteLength,
  ]);

  prefix[1] = 99;
  const whole = new Uint8Array(await blob.arrayBuffer());
  const tail = blob.slice(-6, blob.size, "APPLICATION/OCTET-STREAM");
  const tailBytes = new Uint8Array(await tail.arrayBuffer());
  assertFixture(whole[1] === spec.variant, "Blob copied typed-array bytes at construction");
  assertFixture(tail.type === "application/octet-stream", "Blob slice normalized its type");
  output(
    root,
    "blob-consumed",
    `${Array.from(whole).join("|")}\n${Array.from(tailBytes).join("|")}\n${tail.type}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "blob-consumed", [
    whole.byteLength,
    tail.size,
    tail.type,
    whole[1],
  ]);

  return [
    fact("size", blob.size),
    fact("type", blob.type),
    fact("bytes", Array.from(whole).join("|")),
    fact("tail", Array.from(tailBytes).join("|")),
  ];
}

async function fileMetadataFormData(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const modified = 1_700_000_000_000 + spec.seed;
  const file = new File(
    [`owner=${meta.framework}\n`, new Uint8Array([spec.variant, 10, 255])],
    `résumé-${spec.variant}.txt`,
    { type: "Text/Plain", lastModified: modified },
  );
  assertFixture(file.name === `résumé-${spec.variant}.txt`, "File retained its name");
  assertFixture(file.type === "text/plain", "File normalized its MIME type");
  output(
    root,
    "file-metadata",
    `${file.name}\n${file.type}\n${file.size}\n${file.lastModified}`,
  );
  await capturePlatformStep(host, capture, "platform-1", "file-created", [
    file.name,
    file.type,
    file.size,
    file.lastModified,
  ]);

  const data = new FormData();
  data.append("upload", file);
  data.append("tag", "alpha");
  data.append("tag", `variant-${spec.variant}`);
  const stored = data.get("upload");
  assertFixture(stored instanceof File, "FormData retained the File brand");
  assertFixture(stored === file, "FormData retained the appended File identity");
  const text = await stored.text();
  const entries = Array.from(data.entries(), ([key, value]) =>
    `${key}:${value instanceof File ? value.name : value}`,
  );
  output(root, "formdata-file", `${entries.join("|")}\n${text}\n${data.getAll("tag").join("|")}`);
  await capturePlatformStep(host, capture, "platform-2", "file-in-formdata", [
    entries.join(","),
    data.getAll("tag").join(","),
    text.length,
  ]);

  return [
    fact("name", file.name),
    fact("last-modified", file.lastModified),
    fact("identity", stored === file),
    fact("entries", entries.join("|")),
    fact("text", text),
  ];
}

async function fileReaderEventSequence(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const reader = new FileReader();
  const events: string[] = [];
  for (const type of ["loadstart", "progress", "load", "error", "abort", "loadend"] as const) {
    reader.addEventListener(type, (event) => {
      const progress = event as ProgressEvent<FileReader>;
      events.push(`${type}:${progress.loaded}:${progress.total}:${reader.readyState}`);
    });
  }
  const finished = withEventTimeout(
    new Promise<void>((resolve) => reader.addEventListener("loadend", () => resolve(), { once: true })),
    "FileReader loadend",
  );
  const blob = new Blob([
    meta.framework,
    ":",
    String(spec.seed),
    ":",
    new Uint8Array([0, 127, 255]),
  ]);
  reader.readAsArrayBuffer(blob);
  output(root, "reader-started", `${reader.readyState}\n${events.join("|")}\nsize=${blob.size}`);
  await capturePlatformStep(host, capture, "platform-1", "filereader-started", [
    reader.readyState,
    blob.size,
    events.join(","),
  ]);

  await finished;
  assertFixture(reader.readyState === FileReader.DONE, "FileReader reached DONE");
  assertFixture(reader.error === null, "FileReader completed without error");
  assertFixture(reader.result instanceof ArrayBuffer, "FileReader produced an ArrayBuffer");
  const resultBytes = bytes(reader.result);
  output(root, "reader-finished", `${reader.readyState}\n${events.join("|")}\n${resultBytes.join("|")}`);
  await capturePlatformStep(host, capture, "platform-2", "filereader-finished", [
    reader.readyState,
    events.join(","),
    resultBytes.length,
  ]);

  return [
    fact("events", events.join("|")),
    fact("bytes", resultBytes.join("|")),
    fact("error", reader.error === null),
    fact("ready-state", reader.readyState),
  ];
}

async function fileReaderAbortReuse(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const reader = new FileReader();
  const firstEvents: string[] = [];
  reader.addEventListener("loadstart", () => firstEvents.push("loadstart"));
  reader.addEventListener("abort", () => firstEvents.push("abort"));
  reader.addEventListener("load", () => firstEvents.push("load"));
  reader.addEventListener("loadend", () => firstEvents.push("loadend"));
  const large = new Blob([new Uint8Array(256 * 1024).fill(spec.variant)]);
  reader.readAsText(large);
  reader.abort();
  assertFixture(reader.readyState === FileReader.DONE, "abort moved FileReader to DONE");
  assertFixture(reader.result === null, "abort cleared FileReader result");
  output(root, "reader-aborted", `${firstEvents.join("|")}\n${reader.readyState}\n${reader.result}`);
  await capturePlatformStep(host, capture, "platform-1", "filereader-aborted", [
    firstEvents.join(","),
    reader.readyState,
    reader.result,
  ]);

  const secondEvents: string[] = [];
  const secondFinished = withEventTimeout(
    new Promise<void>((resolve) => {
      reader.addEventListener("load", () => secondEvents.push("load"), { once: true });
      reader.addEventListener(
        "loadend",
        () => {
          secondEvents.push("loadend");
          resolve();
        },
        { once: true },
      );
    }),
    "reused FileReader loadend",
  );
  reader.readAsDataURL(new Blob([`${meta.framework}:${spec.seed}`], { type: "text/plain" }));
  await secondFinished;
  const result = String(reader.result);
  assertFixture(result.startsWith("data:text/plain;base64,"), "reused reader produced a data URL");
  output(root, "reader-reused", `${secondEvents.join("|")}\n${reader.readyState}\n${result}`);
  await capturePlatformStep(host, capture, "platform-2", "filereader-reused", [
    secondEvents.join(","),
    reader.readyState,
    result,
  ]);

  return [
    fact("abort-events", firstEvents.join("|")),
    fact("reuse-events", secondEvents.join("|")),
    fact("result", result),
    fact("error", reader.error === null),
  ];
}

async function structuredCloneBinaryTransfer(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const buffer = new ArrayBuffer(12);
  const view = new DataView(buffer);
  view.setUint32(0, 0x01020304, false);
  view.setInt16(4, -spec.seed, true);
  const words = new Uint16Array(buffer, 6, 3);
  words.set([spec.variant, 500, 65535]);
  output(
    root,
    "transfer-before",
    `${buffer.byteLength}\n${bytes(buffer).join("|")}\n${view.getUint32(0, false).toString(16)}`,
  );
  await capturePlatformStep(host, capture, "platform-1", "binary-before-transfer", [
    buffer.byteLength,
    bytes(buffer).join(","),
    words.join(","),
  ]);

  const source = {
    owner: meta.framework,
    buffer,
    view,
    words,
    map: new Map<string, ArrayBufferView>([["words", words]]),
  };
  const cloned = structuredClone(source, { transfer: [buffer] });
  assertFixture(buffer.byteLength === 0, "structuredClone detached the transferred source buffer");
  assertFixture(cloned.buffer.byteLength === 12, "clone retained transferred bytes");
  assertFixture(cloned.view.buffer === cloned.buffer, "cloned DataView shared cloned buffer identity");
  assertFixture(cloned.words.buffer === cloned.buffer, "cloned typed array shared cloned buffer identity");
  const mapWords = cloned.map.get("words");
  assertFixture(mapWords instanceof Uint16Array, "cloned Map retained typed-array brand");
  assertFixture(mapWords.buffer === cloned.buffer, "Map value retained cloned buffer aliasing");
  output(
    root,
    "transfer-after",
    `${buffer.byteLength}\n${bytes(cloned.buffer).join("|")}\n${cloned.words.join("|")}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "binary-after-transfer", [
    buffer.byteLength,
    cloned.buffer.byteLength,
    bytes(cloned.buffer).join(","),
    cloned.words.join(","),
  ]);

  return [
    fact("source-detached", buffer.byteLength),
    fact("clone-bytes", bytes(cloned.buffer).join("|")),
    fact("words", cloned.words.join("|")),
    fact("aliases", cloned.view.buffer === cloned.words.buffer),
    fact("map-brand", mapWords instanceof Uint16Array),
  ];
}

async function dataUrlFetchBinary(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const textUrl = `data:text/plain;charset=utf-8,${encodeURIComponent(`${meta.framework}:café:東京:${spec.seed}`)}`;
  const binaryUrl = "data:application/octet-stream;base64,AAECf4D+/w==";
  const textResponse = await fetch(textUrl);
  const binaryResponse = await fetch(binaryUrl);
  assertFixture(textResponse.ok && binaryResponse.ok, "data URL responses succeeded");
  output(
    root,
    "data-responses",
    `${textResponse.status}:${textResponse.headers.get("content-type")}\n${binaryResponse.status}:${binaryResponse.headers.get("content-type")}\n${textResponse.bodyUsed}:${binaryResponse.bodyUsed}`,
  );
  await capturePlatformStep(host, capture, "platform-1", "data-url-responses", [
    textResponse.status,
    textResponse.headers.get("content-type"),
    binaryResponse.headers.get("content-type"),
  ]);

  const [text, binary] = await Promise.all([
    textResponse.text(),
    binaryResponse.arrayBuffer(),
  ]);
  assertFixture(text === `${meta.framework}:café:東京:${spec.seed}`, "data URL decoded Unicode text");
  assertFixture(bytes(binary).join("|") === "0|1|2|127|128|254|255", "base64 data URL decoded bytes");
  output(
    root,
    "data-consumed",
    `${text}\n${bytes(binary).join("|")}\n${textResponse.bodyUsed}:${binaryResponse.bodyUsed}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "data-url-consumed", [
    text,
    bytes(binary).join(","),
    textResponse.bodyUsed,
    binaryResponse.bodyUsed,
  ]);

  return [
    fact("text", text),
    fact("bytes", bytes(binary).join("|")),
    fact("text-type", textResponse.headers.get("content-type")),
    fact("binary-type", binaryResponse.headers.get("content-type")),
  ];
}

async function responseBinaryClone(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const file = new File(
    [meta.framework, ":", new Uint8Array([spec.seed & 255, spec.variant, 255])],
    `payload-${spec.variant}.bin`,
    { type: "application/x-smoke-binary", lastModified: 1_710_000_000_000 + spec.seed },
  );
  const response = new Response(file, {
    status: 202,
    statusText: "Accepted Fixture",
    headers: { "X-Smoke-Owner": meta.framework },
  });
  const clone = response.clone();
  output(
    root,
    "response-created",
    `${response.status}:${response.statusText}\n${response.headers.get("content-type")}\n${response.bodyUsed}:${clone.bodyUsed}`,
  );
  await capturePlatformStep(host, capture, "platform-1", "binary-response-created", [
    response.status,
    response.statusText,
    response.headers.get("content-type"),
    response.headers.get("x-smoke-owner"),
  ]);

  const [text, cloneBlob] = await Promise.all([response.text(), clone.blob()]);
  const cloneBytes = new Uint8Array(await cloneBlob.arrayBuffer());
  assertFixture(cloneBlob.type === file.type, "Response clone retained body MIME type");
  assertFixture(response.bodyUsed && clone.bodyUsed, "both response branches became consumed");
  output(
    root,
    "response-consumed",
    `${text}\n${Array.from(cloneBytes).join("|")}\n${cloneBlob.type}\n${response.bodyUsed}:${clone.bodyUsed}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "binary-response-consumed", [
    text.length,
    cloneBytes.byteLength,
    cloneBlob.type,
    response.bodyUsed,
    clone.bodyUsed,
  ]);

  return [
    fact("status", response.status),
    fact("text", text),
    fact("clone-bytes", Array.from(cloneBytes).join("|")),
    fact("type", cloneBlob.type),
    fact("body-used", `${response.bodyUsed}:${clone.bodyUsed}`),
  ];
}

const SCENARIOS: Record<string, BinaryScenario> = {
  "url-resolution-mutation": urlResolutionMutation,
  "searchparams-live-binding": searchParamsLiveBinding,
  "searchparams-iterator-mutation": searchParamsIteratorMutation,
  "blob-unicode-slice": blobUnicodeSlice,
  "file-metadata-formdata": fileMetadataFormData,
  "filereader-event-sequence": fileReaderEventSequence,
  "filereader-abort-reuse": fileReaderAbortReuse,
  "structured-clone-binary-transfer": structuredCloneBinaryTransfer,
  "data-url-fetch-binary": dataUrlFetchBinary,
  "response-binary-clone": responseBinaryClone,
};

export async function runUrlFileBinaryBoundaryCase(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformBoundaryResult> {
  const scenario = SCENARIOS[spec.slug];
  assertFixture(scenario, `missing URL/file/binary scenario ${spec.slug}`);
  try {
    const facts = await scenario(host, meta, spec, capture);
    return { status: "ready", facts };
  } catch (error: unknown) {
    throw new Error(`${errorName(error)}: ${error instanceof Error ? error.message : String(error)}`);
  }
}

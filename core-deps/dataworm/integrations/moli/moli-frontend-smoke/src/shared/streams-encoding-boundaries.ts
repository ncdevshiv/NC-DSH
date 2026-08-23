import { assertFixture } from "./harness";
import {
  capturePlatformStep,
  fact,
  type CapturePlatformFrame,
  type PlatformBoundaryResult,
  type PlatformFact,
  withEventTimeout,
} from "./platform-boundary";
import type { CaseSpec, SmokeMeta } from "./types";

type StreamsScenario = (
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
) => Promise<PlatformFact[]>;

function scenarioRoot(host: HTMLElement, name: string): HTMLElement {
  const root = document.createElement("section");
  root.dataset.streamScenario = name;
  host.append(root);
  return root;
}

function output(root: HTMLElement, name: string, value: string): HTMLOutputElement {
  const element = document.createElement("output");
  element.dataset.streamOutput = name;
  element.textContent = value;
  root.append(element);
  return element;
}

function errorName(error: unknown): string {
  return error instanceof Error || error instanceof DOMException
    ? error.name
    : Object.prototype.toString.call(error);
}

async function readAll<T>(reader: ReadableStreamDefaultReader<T>): Promise<T[]> {
  const values: T[] = [];
  while (true) {
    const result = await reader.read();
    if (result.done) {
      return values;
    }
    values.push(result.value);
  }
}

async function readableControllerPull(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const lifecycle: string[] = [];
  let pullCount = 0;
  const stream = new ReadableStream<string>({
    start(controller) {
      lifecycle.push(`start:${String(controller.desiredSize)}`);
      controller.enqueue(`${meta.framework}:alpha`);
    },
    pull(controller) {
      pullCount += 1;
      lifecycle.push(`pull-${pullCount}:${String(controller.desiredSize)}`);
      if (pullCount === 1) {
        controller.enqueue(`seed:${spec.seed}`);
        return;
      }
      controller.enqueue(`variant:${spec.variant}`);
      controller.close();
      lifecycle.push("close");
    },
  });
  const unlockedInitially = !stream.locked;
  const reader = stream.getReader();
  const first = await reader.read();
  assertFixture(!first.done, "ReadableStream produced its start chunk");
  assertFixture(first.value === `${meta.framework}:alpha`, "start chunk retained framework input");
  output(
    root,
    "readable-first",
    `${first.value}\nlocked=${stream.locked}\n${lifecycle.join("|")}`,
  );
  await capturePlatformStep(host, capture, "platform-1", "readable-first-chunk", [
    first.value,
    stream.locked,
    lifecycle.join("|"),
  ]);

  const remaining = await readAll(reader);
  await reader.closed;
  assertFixture(
    remaining.join("|") === `seed:${spec.seed}|variant:${spec.variant}`,
    "ReadableStream pull chunks retained order",
  );
  const lockedBeforeRelease = stream.locked;
  reader.releaseLock();
  output(
    root,
    "readable-complete",
    `${remaining.join("|")}\nlocked=${lockedBeforeRelease}>${stream.locked}\n${lifecycle.join("|")}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "readable-stream-closed", [
    remaining.join("|"),
    lockedBeforeRelease,
    stream.locked,
    lifecycle.join("|"),
  ]);

  return [
    fact("unlocked-initially", unlockedInitially),
    fact("first", first.value),
    fact("remaining", remaining.join("|")),
    fact("pull-count", pullCount),
    fact("lifecycle", lifecycle.join("|")),
    fact("unlocked-finally", !stream.locked),
  ];
}

async function readableCancelReason(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const lifecycle: string[] = [];
  let observedReason: unknown;
  let resolveCanceled: (() => void) | undefined;
  const canceled = withEventTimeout(
    new Promise<void>((resolve) => {
      resolveCanceled = resolve;
    }),
    "ReadableStream cancel callback",
  );
  const stream = new ReadableStream<string>({
    start(controller) {
      lifecycle.push("start");
      controller.enqueue(`${meta.framework}-${spec.variant}`);
      controller.enqueue(`queued-${spec.seed}`);
    },
    cancel(reason) {
      observedReason = reason;
      lifecycle.push(`cancel:${String((reason as { code?: string }).code ?? reason)}`);
      resolveCanceled?.();
    },
  });
  const reader = stream.getReader();
  const first = await reader.read();
  assertFixture(!first.done, "cancel scenario read its first chunk");
  output(root, "cancel-before", `${first.value}\nlocked=${stream.locked}\n${lifecycle.join("|")}`);
  await capturePlatformStep(host, capture, "platform-1", "readable-before-cancel", [
    first.value,
    stream.locked,
    lifecycle.join("|"),
  ]);

  const reason = { code: `stop-${meta.framework}`, variant: spec.variant };
  await reader.cancel(reason);
  await canceled;
  await reader.closed;
  assertFixture(observedReason === reason, "underlying source received the exact cancel reason");
  const afterCancel = await reader.read();
  assertFixture(afterCancel.done, "canceled reader remained closed");
  reader.releaseLock();
  output(
    root,
    "cancel-after",
    `${String(afterCancel.done)}\nreason=${reason.code}:${reason.variant}\nlocked=${stream.locked}\n${lifecycle.join("|")}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "readable-canceled", [
    afterCancel.done,
    reason.code,
    reason.variant,
    stream.locked,
    lifecycle.join("|"),
  ]);

  return [
    fact("first", first.value),
    fact("cancel-reason-identity", observedReason === reason),
    fact("cancel-code", reason.code),
    fact("closed-read", afterCancel.done),
    fact("lifecycle", lifecycle.join("|")),
  ];
}

async function teeBranchConsumption(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const sourceLifecycle: string[] = [];
  const chunks = [
    `${meta.framework}:head`,
    `seed:${spec.seed}`,
    `variant:${spec.variant}`,
  ];
  const source = new ReadableStream<string>({
    start(controller) {
      sourceLifecycle.push("start");
      for (const chunk of chunks) {
        controller.enqueue(chunk);
        sourceLifecycle.push(`enqueue:${chunk}`);
      }
      controller.close();
      sourceLifecycle.push("close");
    },
    cancel(reason) {
      sourceLifecycle.push(`source-cancel:${String(reason)}`);
    },
  });
  const [left, right] = source.tee();
  const leftReader = left.getReader();
  const rightReader = right.getReader();
  const leftFirst = await leftReader.read();
  assertFixture(leftFirst.value === chunks[0], "left tee branch received the first chunk");
  output(
    root,
    "tee-first",
    `${leftFirst.value}\nsourceLocked=${source.locked}\nleftLocked=${left.locked}\nrightLocked=${right.locked}`,
  );
  await capturePlatformStep(host, capture, "platform-1", "tee-left-first", [
    leftFirst.value,
    source.locked,
    left.locked,
    right.locked,
  ]);

  const leftCancel = leftReader.cancel("left-stop");
  const rightValues = await readAll(rightReader);
  await leftCancel;
  const leftAfterCancel = await leftReader.read();
  assertFixture(leftAfterCancel.done, "canceled tee branch closed");
  assertFixture(rightValues.join("|") === chunks.join("|"), "right tee branch retained every chunk");
  assertFixture(
    !sourceLifecycle.some((item) => item.startsWith("source-cancel:")),
    "consuming the other tee branch avoided source cancellation",
  );
  leftReader.releaseLock();
  rightReader.releaseLock();
  output(
    root,
    "tee-complete",
    `${rightValues.join("|")}\nleftDone=${leftAfterCancel.done}\nsource=${sourceLifecycle.join("|")}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "tee-branches-complete", [
    rightValues.join("|"),
    leftAfterCancel.done,
    sourceLifecycle.join("|"),
  ]);

  return [
    fact("left-first", leftFirst.value),
    fact("left-canceled", leftAfterCancel.done),
    fact("right-values", rightValues.join("|")),
    fact("source-canceled", sourceLifecycle.some((item) => item.startsWith("source-cancel:"))),
    fact("source-lifecycle", sourceLifecycle.join("|")),
  ];
}

async function transformPipeBackpressure(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const lifecycle: string[] = [];
  const chunks = [meta.framework, `seed-${spec.seed}`, `variant-${spec.variant}`];
  let sourceIndex = 0;
  const source = new ReadableStream<string>(
    {
      pull(controller) {
        const chunk = chunks[sourceIndex];
        if (chunk === undefined) {
          lifecycle.push("source-close");
          controller.close();
          return;
        }
        sourceIndex += 1;
        lifecycle.push(`source:${chunk}:${String(controller.desiredSize)}`);
        controller.enqueue(chunk);
      },
    },
    { highWaterMark: 1 },
  );
  const transform = new TransformStream<string, string>(
    {
      transform(chunk, controller) {
        lifecycle.push(`transform:${chunk}:${String(controller.desiredSize)}`);
        controller.enqueue(`${sourceIndex}:${chunk.toUpperCase()}`);
      },
      flush(controller) {
        lifecycle.push(`flush:${String(controller.desiredSize)}`);
        controller.enqueue("FLUSH");
      },
    },
    { highWaterMark: 1 },
    { highWaterMark: 1 },
  );
  const piped = source.pipeThrough(transform);
  const reader = piped.getReader();
  const first = await reader.read();
  assertFixture(!first.done, "pipeThrough produced a transformed first chunk");
  assertFixture(first.value.endsWith(meta.framework.toUpperCase()), "transform uppercased the first chunk");
  output(root, "pipe-first", `${first.value}\n${lifecycle.join("|")}\nlocked=${source.locked}:${piped.locked}`);
  await capturePlatformStep(host, capture, "platform-1", "transform-first-output", [
    first.value,
    lifecycle.join("|"),
    source.locked,
    piped.locked,
  ]);

  const remaining = await readAll(reader);
  await reader.closed;
  assertFixture(remaining.at(-1) === "FLUSH", "TransformStream flush emitted its terminal chunk");
  assertFixture(remaining.length === 3, "transform produced two remaining chunks and flush output");
  reader.releaseLock();
  output(
    root,
    "pipe-complete",
    `${remaining.join("|")}\n${lifecycle.join("|")}\nlocked=${source.locked}:${piped.locked}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "transform-pipe-complete", [
    remaining.join("|"),
    lifecycle.join("|"),
    source.locked,
    piped.locked,
  ]);

  return [
    fact("first", first.value),
    fact("remaining", remaining.join("|")),
    fact("lifecycle", lifecycle.join("|")),
    fact("source-unlocked", !source.locked),
    fact("output-unlocked", !piped.locked),
  ];
}

async function writableCloseAbort(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const lifecycle: string[] = [];
  const stored: string[] = [];
  const writable = new WritableStream<string>(
    {
      start(controller) {
        lifecycle.push(`start:${controller.signal.aborted}`);
        controller.signal.addEventListener("abort", () => lifecycle.push("signal-abort"));
      },
      write(chunk, controller) {
        lifecycle.push(`write:${chunk}:${controller.signal.aborted}`);
        stored.push(chunk);
      },
      close() {
        lifecycle.push("close");
      },
      abort(reason) {
        lifecycle.push(`abort:${String(reason)}`);
      },
    },
    { highWaterMark: 2, size: (chunk) => chunk.length },
  );
  const writer = writable.getWriter();
  const desiredBefore = writer.desiredSize;
  await writer.write(meta.framework);
  await writer.write(`v${spec.variant}`);
  output(
    root,
    "writable-written",
    `${stored.join("|")}\ndesired=${String(desiredBefore)}>${String(writer.desiredSize)}\n${lifecycle.join("|")}`,
  );
  await capturePlatformStep(host, capture, "platform-1", "writable-written", [
    stored.join("|"),
    desiredBefore,
    writer.desiredSize,
    lifecycle.join("|"),
  ]);

  await writer.close();
  await writer.closed;
  const closedLifecycle = lifecycle.join("|");
  writer.releaseLock();
  assertFixture(!writable.locked, "closed WritableStream unlocked after releaseLock");
  output(root, "writable-closed", `${closedLifecycle}\nlocked=${writable.locked}`);
  await capturePlatformStep(host, capture, "platform-2", "writable-closed", [
    closedLifecycle,
    writable.locked,
    stored.length,
  ]);

  const abortLifecycle: string[] = [];
  const aborted = new WritableStream<string>({
    start(controller) {
      controller.signal.addEventListener("abort", () => {
        abortLifecycle.push(`signal:${String(controller.signal.reason)}`);
      });
    },
    write(chunk) {
      abortLifecycle.push(`write:${chunk}`);
    },
    abort(reason) {
      abortLifecycle.push(`sink:${String(reason)}`);
    },
  });
  const abortWriter = aborted.getWriter();
  await abortWriter.write(`abort-${spec.seed}`);
  await abortWriter.abort("fixture-stop");
  let closedError = "missing";
  try {
    await abortWriter.closed;
  } catch (error: unknown) {
    closedError = errorName(error);
  }
  abortWriter.releaseLock();
  output(root, "writable-aborted", `${abortLifecycle.join("|")}\nclosed=${closedError}`);
  return [
    fact("stored", stored.join("|")),
    fact("closed-lifecycle", closedLifecycle),
    fact("abort-lifecycle", abortLifecycle.join("|")),
    fact("abort-closed-error", closedError),
    fact("closed-unlocked", !writable.locked),
  ];
}

async function decoderSplitCodepoints(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const text = `A€😀B-${spec.variant}`;
  const bytes = new TextEncoder().encode(text);
  const decoder = new TextDecoder("utf-8");
  const pieces: string[] = [];
  pieces.push(decoder.decode(bytes.slice(0, 2), { stream: true }));
  pieces.push(decoder.decode(bytes.slice(2, 3), { stream: true }));
  const partial = pieces.join("");
  assertFixture(partial === "A", "split decoder buffered an incomplete euro code point");
  output(
    root,
    "decode-partial",
    `${partial}\npieces=${pieces.map((item) => JSON.stringify(item)).join("|")}\nbytes=${Array.from(bytes.slice(0, 3)).join("|")}`,
  );
  await capturePlatformStep(host, capture, "platform-1", "decoder-buffered-codepoint", [
    partial,
    pieces.map((item) => JSON.stringify(item)).join("|"),
    Array.from(bytes.slice(0, 3)).join("|"),
    decoder.encoding,
  ]);

  pieces.push(decoder.decode(bytes.slice(3, 6), { stream: true }));
  pieces.push(decoder.decode(bytes.slice(6), { stream: true }));
  pieces.push(decoder.decode());
  const decoded = pieces.join("");
  assertFixture(decoded === text, "streaming decoder reassembled split multibyte code points");
  output(
    root,
    "decode-complete",
    `${decoded}\npieces=${pieces.map((item) => JSON.stringify(item)).join("|")}\nbytes=${bytes.byteLength}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "decoder-codepoints-complete", [
    decoded,
    pieces.map((item) => JSON.stringify(item)).join("|"),
    bytes.byteLength,
    decoder.fatal,
    decoder.ignoreBOM,
  ]);

  return [
    fact("decoded", decoded),
    fact("pieces", pieces.map((item) => JSON.stringify(item)).join("|")),
    fact("byte-length", bytes.byteLength),
    fact("encoding", decoder.encoding),
    fact("fatal", decoder.fatal),
  ];
}

async function decoderFatalRecovery(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const fatal = new TextDecoder("utf-8", { fatal: true });
  let fatalError = "missing";
  try {
    fatal.decode(new Uint8Array([0xc3, 0x28]));
  } catch (error: unknown) {
    fatalError = errorName(error);
  }
  assertFixture(fatalError === "TypeError", "fatal TextDecoder rejected malformed UTF-8");
  const labels: string[] = [];
  for (const label of ["utf8", "unicode-1-1-utf-8", "UTF-8"]) {
    labels.push(new TextDecoder(label).encoding);
  }
  output(root, "fatal-error", `${fatalError}\n${labels.join("|")}\nfatal=${fatal.fatal}`);
  await capturePlatformStep(host, capture, "platform-1", "decoder-fatal-error", [
    fatalError,
    labels.join("|"),
    fatal.fatal,
  ]);

  const replacement = new TextDecoder().decode(new Uint8Array([0xc3, 0x28]));
  const windows = new TextDecoder("windows-1252").decode(
    new Uint8Array([0x80, 0x20, 0x63, 0x61, 0x66, 0xe9]),
  );
  const bomBytes = new Uint8Array([0xef, 0xbb, 0xbf, 0x41, spec.variant + 48]);
  const bomRemoved = new TextDecoder("utf-8").decode(bomBytes);
  const bomRetained = new TextDecoder("utf-8", { ignoreBOM: true }).decode(bomBytes);
  assertFixture(replacement === "�(", "non-fatal decoder emitted a replacement code point");
  assertFixture(windows === "€ café", "windows-1252 decoder mapped legacy bytes");
  assertFixture(bomRemoved === `A${spec.variant}`, "default UTF-8 decoder removed BOM");
  assertFixture(bomRetained === `\uFEFFA${spec.variant}`, "ignoreBOM retained BOM in output");
  output(
    root,
    "decoder-recovery",
    `${replacement}\n${windows}\n${JSON.stringify(bomRemoved)}|${JSON.stringify(bomRetained)}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "decoder-recovered", [
    replacement,
    windows,
    JSON.stringify(bomRemoved),
    JSON.stringify(bomRetained),
  ]);

  return [
    fact("fatal-error", fatalError),
    fact("labels", labels.join("|")),
    fact("replacement", replacement),
    fact("windows-1252", windows),
    fact("bom-removed", JSON.stringify(bomRemoved)),
    fact("bom-retained", JSON.stringify(bomRetained)),
  ];
}

async function encodingStreamPipeline(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const inputs = [
    `${meta.framework}:`,
    `café-${spec.seed}:`,
    `東京-${spec.variant}`,
  ];
  let index = 0;
  const source = new ReadableStream<string>({
    pull(controller) {
      const value = inputs[index];
      if (value === undefined) {
        controller.close();
        return;
      }
      index += 1;
      controller.enqueue(value);
    },
  });
  const encoder = new TextEncoderStream();
  const decoder = new TextDecoderStream("utf-8", { fatal: true, ignoreBOM: false });
  const pipeline = source.pipeThrough(encoder).pipeThrough(decoder);
  const reader = pipeline.getReader();
  const first = await reader.read();
  assertFixture(first.value === inputs[0], "encoding pipeline retained the first chunk");
  output(
    root,
    "encoding-first",
    `${first.value}\n${encoder.encoding}:${decoder.encoding}\n${decoder.fatal}:${decoder.ignoreBOM}`,
  );
  await capturePlatformStep(host, capture, "platform-1", "encoding-stream-first", [
    first.value,
    encoder.encoding,
    decoder.encoding,
    decoder.fatal,
    decoder.ignoreBOM,
  ]);

  const remaining = await readAll(reader);
  const combined = [first.value, ...remaining].join("");
  assertFixture(combined === inputs.join(""), "TextEncoderStream/TextDecoderStream roundtripped text");
  reader.releaseLock();
  output(root, "encoding-complete", `${remaining.join("|")}\ncombined=${combined}\nlocked=${pipeline.locked}`);
  await capturePlatformStep(host, capture, "platform-2", "encoding-stream-complete", [
    remaining.join("|"),
    combined,
    pipeline.locked,
    source.locked,
  ]);

  return [
    fact("first", first.value),
    fact("remaining", remaining.join("|")),
    fact("combined", combined),
    fact("encoder", encoder.encoding),
    fact("decoder", decoder.encoding),
    fact("source-unlocked", !source.locked),
  ];
}

async function byteStreamByob(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const sourceBytes = new Uint8Array([
    1,
    2,
    spec.variant + 3,
    127,
    128,
    254,
    255,
  ]);
  const lifecycle: string[] = [];
  const stream = new ReadableStream({
    type: "bytes",
    start(controller) {
      lifecycle.push(`start:${String(controller.desiredSize)}`);
      controller.enqueue(sourceBytes);
      lifecycle.push(`enqueue:${sourceBytes.byteLength}`);
      controller.close();
      lifecycle.push("close");
    },
  });
  const reader = stream.getReader({ mode: "byob" });
  const firstBuffer = new ArrayBuffer(3);
  const first = await reader.read(new Uint8Array(firstBuffer));
  const firstValues = Array.from(first.value ?? []).join("|");
  assertFixture(!first.done, "BYOB reader returned its first bytes");
  assertFixture(firstValues === `1|2|${spec.variant + 3}`, "BYOB first read filled the supplied capacity");
  output(
    root,
    "byob-first",
    `${firstValues}\ninputBuffer=${firstBuffer.byteLength}\nreturned=${first.value?.byteLength}\n${lifecycle.join("|")}`,
  );
  await capturePlatformStep(host, capture, "platform-1", "byob-first-read", [
    firstValues,
    firstBuffer.byteLength,
    first.value?.byteLength,
    stream.locked,
    lifecycle.join("|"),
  ]);

  const secondBuffer = new ArrayBuffer(8);
  const second = await reader.read(new Uint8Array(secondBuffer));
  const secondValues = Array.from(second.value ?? []).join("|");
  const terminalBuffer = new ArrayBuffer(2);
  const terminal = await reader.read(new Uint8Array(terminalBuffer));
  assertFixture(secondValues === "127|128|254|255", "BYOB second read returned remaining bytes");
  assertFixture(terminal.done, "BYOB reader reported the closed stream");
  const terminalLength = terminal.value?.byteLength ?? -1;
  reader.releaseLock();
  output(
    root,
    "byob-complete",
    `${secondValues}\nsecondInput=${secondBuffer.byteLength}\nterminal=${terminal.done}:${terminalLength}:${terminalBuffer.byteLength}\nlocked=${stream.locked}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "byob-stream-complete", [
    secondValues,
    secondBuffer.byteLength,
    terminal.done,
    terminalLength,
    terminalBuffer.byteLength,
    stream.locked,
  ]);

  return [
    fact("first", firstValues),
    fact("first-input-detached", firstBuffer.byteLength),
    fact("second", secondValues),
    fact("second-input-detached", secondBuffer.byteLength),
    fact("terminal", `${terminal.done}:${terminalLength}`),
    fact("unlocked", !stream.locked),
  ];
}

async function blobResponseStream(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const expected = `prefix:${meta.framework}:café:東京:${spec.seed}:${spec.variant}:tail`;
  const encoded = new TextEncoder().encode(`café:東京:${spec.seed}`);
  const blob = new Blob(
    ["prefix:", meta.framework, ":", encoded, `:${spec.variant}:tail`],
    { type: "text/plain;charset=UTF-8" },
  );
  const response = new Response(blob, {
    status: 202,
    statusText: "Accepted",
    headers: { "X-Stream-Owner": meta.framework },
  });
  const clone = response.clone();
  const slice = blob.slice(7, 7 + meta.framework.length, "text/custom");
  assertFixture(!response.bodyUsed && !clone.bodyUsed, "Response clone bodies began unused");
  output(
    root,
    "blob-created",
    `${blob.size}:${blob.type}\n${slice.size}:${slice.type}\n${response.status}:${response.statusText}:${response.headers.get("x-stream-owner")}\nused=${response.bodyUsed}:${clone.bodyUsed}`,
  );
  await capturePlatformStep(host, capture, "platform-1", "blob-response-created", [
    blob.size,
    blob.type,
    slice.size,
    slice.type,
    response.status,
    response.statusText,
    response.bodyUsed,
    clone.bodyUsed,
  ]);

  const [responseText, cloneBytes, streamText, sliceText] = await Promise.all([
    response.text(),
    clone.arrayBuffer(),
    new Response(blob.stream()).text(),
    slice.text(),
  ]);
  const cloneText = new TextDecoder().decode(cloneBytes);
  assertFixture(responseText === expected, "Response decoded the Blob body");
  assertFixture(cloneText === expected, "Response clone returned identical bytes");
  assertFixture(streamText === expected, "Blob.stream returned identical text");
  assertFixture(sliceText === meta.framework, "Blob.slice retained the selected framework bytes");
  output(
    root,
    "blob-consumed",
    `${responseText}\nclone=${cloneText}\nstream=${streamText}\nslice=${sliceText}\nused=${response.bodyUsed}:${clone.bodyUsed}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "blob-response-consumed", [
    responseText,
    cloneText,
    streamText,
    sliceText,
    cloneBytes.byteLength,
    response.bodyUsed,
    clone.bodyUsed,
  ]);

  return [
    fact("text", responseText),
    fact("clone-equal", cloneText === responseText),
    fact("stream-equal", streamText === responseText),
    fact("slice", sliceText),
    fact("bytes", cloneBytes.byteLength),
    fact("body-used", `${response.bodyUsed}:${clone.bodyUsed}`),
  ];
}

const SCENARIOS: Record<string, StreamsScenario> = {
  "readable-controller-pull": readableControllerPull,
  "readable-cancel-reason": readableCancelReason,
  "tee-branch-consumption": teeBranchConsumption,
  "transform-pipe-backpressure": transformPipeBackpressure,
  "writable-close-abort": writableCloseAbort,
  "decoder-split-codepoints": decoderSplitCodepoints,
  "decoder-fatal-recovery": decoderFatalRecovery,
  "encoding-stream-pipeline": encodingStreamPipeline,
  "byte-stream-byob": byteStreamByob,
  "blob-response-stream": blobResponseStream,
};

export async function runStreamsEncodingBoundaryCase(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformBoundaryResult> {
  const scenario = SCENARIOS[spec.slug];
  assertFixture(scenario, `missing streams/encoding scenario ${spec.slug}`);
  const facts = await scenario(host, meta, spec, capture);
  return { status: "ready", facts };
}

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

type WorkerScenario = (
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
) => Promise<PlatformFact[]>;

type MessageRecord = Record<string, unknown>;

interface BlobWorkerOwner {
  worker: Worker;
  url: string;
}

function scenarioRoot(host: HTMLElement, name: string): HTMLElement {
  const root = document.createElement("section");
  root.dataset.workerScenario = name;
  host.append(root);
  return root;
}

function output(root: HTMLElement, name: string, value: string): HTMLOutputElement {
  const element = document.createElement("output");
  element.dataset.workerOutput = name;
  element.textContent = value;
  root.append(element);
  return element;
}

function tokenFor(meta: SmokeMeta, spec: CaseSpec): string {
  return `${meta.framework}-${spec.seed}-${spec.variant}`;
}

function stableOrigin(origin: string): string {
  if (!origin) {
    return "";
  }
  const url = new URL(origin);
  return `${url.protocol}//${url.hostname}`;
}

function createBlobWorker(source: string, name: string): BlobWorkerOwner {
  const url = URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
  return { worker: new Worker(url, { name }), url };
}

function disposeBlobWorker(owner: BlobWorkerOwner): void {
  owner.worker.terminate();
  URL.revokeObjectURL(owner.url);
}

function nextWorkerMessage<T extends MessageRecord>(
  worker: Worker,
  label: string,
): Promise<MessageEvent<T>> {
  return withEventTimeout(
    new Promise<MessageEvent<T>>((resolve, reject) => {
      const cleanup = (): void => {
        worker.removeEventListener("message", onMessage as EventListener);
        worker.removeEventListener("messageerror", onMessageError as EventListener);
        worker.removeEventListener("error", onError);
      };
      const onMessage = (event: MessageEvent<T>): void => {
        cleanup();
        resolve(event);
      };
      const onMessageError = (): void => {
        cleanup();
        reject(new Error(`${label} emitted messageerror`));
      };
      const onError = (event: ErrorEvent): void => {
        cleanup();
        reject(new Error(`${label} worker error: ${event.message}`));
      };
      worker.addEventListener("message", onMessage as EventListener);
      worker.addEventListener("messageerror", onMessageError as EventListener);
      worker.addEventListener("error", onError);
    }),
    label,
  );
}

function nextPortMessage<T extends MessageRecord>(
  port: MessagePort,
  label: string,
): Promise<MessageEvent<T>> {
  return withEventTimeout(
    new Promise<MessageEvent<T>>((resolve, reject) => {
      const cleanup = (): void => {
        port.removeEventListener("message", onMessage as EventListener);
        port.removeEventListener("messageerror", onMessageError as EventListener);
      };
      const onMessage = (event: MessageEvent<T>): void => {
        cleanup();
        resolve(event);
      };
      const onMessageError = (): void => {
        cleanup();
        reject(new Error(`${label} emitted messageerror`));
      };
      port.addEventListener("message", onMessage as EventListener);
      port.addEventListener("messageerror", onMessageError as EventListener);
    }),
    label,
  );
}

function nextBroadcastMessage<T extends MessageRecord>(
  channel: BroadcastChannel,
  label: string,
): Promise<MessageEvent<T>> {
  return withEventTimeout(
    new Promise<MessageEvent<T>>((resolve, reject) => {
      const cleanup = (): void => {
        channel.removeEventListener("message", onMessage as EventListener);
        channel.removeEventListener("messageerror", onMessageError as EventListener);
      };
      const onMessage = (event: MessageEvent<T>): void => {
        cleanup();
        resolve(event);
      };
      const onMessageError = (): void => {
        cleanup();
        reject(new Error(`${label} emitted messageerror`));
      };
      channel.addEventListener("message", onMessage as EventListener);
      channel.addEventListener("messageerror", onMessageError as EventListener);
    }),
    label,
  );
}

async function dedicatedEventOrder(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const name = `order-${tokenFor(meta, spec)}`;
  const owner = createBlobWorker(
    `const order = ["script"];
queueMicrotask(() => {
  order.push("boot-microtask");
  postMessage({ phase: "boot", order: [...order], name: self.name, protocol: new URL(location.href).protocol });
});
addEventListener("message", (event) => {
  order.push("message:" + event.data.label);
  Promise.resolve().then(() => {
    order.push("reply-microtask");
    postMessage({ phase: "reply", order: [...order], value: event.data.value * 2 });
  });
});`,
    name,
  );
  try {
    const boot = await nextWorkerMessage<MessageRecord>(owner.worker, "dedicated worker boot");
    assertFixture(boot.data.phase === "boot", "dedicated worker published its boot phase");
    assertFixture(boot.data.name === name, "dedicated worker exposed its constructor name");
    assertFixture(boot.data.protocol === "blob:", "blob worker location retained the blob scheme");
    const bootOrder = (boot.data.order as string[]).join("|");
    output(
      root,
      "dedicated-boot",
      `${bootOrder}\norigin=${boot.origin}\nsource=${boot.source === null}\nports=${boot.ports.length}`,
    );
    await capturePlatformStep(host, capture, "platform-1", "worker-boot-microtask", [
      bootOrder,
      boot.origin,
      boot.source === null,
      boot.ports.length,
    ]);

    const replyPromise = nextWorkerMessage<MessageRecord>(owner.worker, "dedicated worker reply");
    owner.worker.postMessage({ label: meta.framework, value: spec.variant + 3 });
    const reply = await replyPromise;
    const replyOrder = (reply.data.order as string[]).join("|");
    assertFixture(reply.data.phase === "reply", "dedicated worker published its reply phase");
    assertFixture(
      replyOrder.endsWith(`message:${meta.framework}|reply-microtask`),
      "worker message callback drained its microtask before replying",
    );
    output(root, "dedicated-reply", `${replyOrder}\nvalue=${reply.data.value}`);
    await capturePlatformStep(host, capture, "platform-2", "worker-message-reply", [
      replyOrder,
      reply.data.value,
    ]);

    return [
      fact("boot-order", bootOrder),
      fact("reply-order", replyOrder),
      fact("message-origin", boot.origin),
      fact("message-source-null", boot.source === null),
      fact("reply", reply.data.value),
    ];
  } finally {
    disposeBlobWorker(owner);
  }
}

async function structuredCloneRichValues(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const owner = createBlobWorker(
    `postMessage({ phase: "ready", cloneType: typeof structuredClone });
addEventListener("message", (event) => {
  const value = event.data;
  postMessage({
    phase: "cloned",
    date: value.date instanceof Date ? value.date.toISOString() : "wrong",
    regexp: value.regexp instanceof RegExp ? value.regexp.source + "/" + value.regexp.flags + ":" + value.regexp.lastIndex : "wrong",
    map: value.map instanceof Map ? Array.from(value.map, ([key, item]) => key + "=" + (typeof item === "object" ? item.count : item)).join("|") : "wrong",
    set: value.set instanceof Set ? Array.from(value.set).join("|") : "wrong",
    cycle: value.cycle.self === value.cycle,
    bigint: String(value.bigint),
    typed: value.typed instanceof Uint16Array ? Array.from(value.typed).join("|") : "wrong",
    hole: 0 in value.sparse,
    undefinedValue: value.sparse[2] === undefined,
  });
});`,
    `clone-${tokenFor(meta, spec)}`,
  );
  try {
    const ready = await nextWorkerMessage<MessageRecord>(owner.worker, "clone worker ready");
    assertFixture(ready.data.cloneType === "function", "worker exposed structuredClone");
    output(root, "clone-ready", `${ready.data.phase}:${ready.data.cloneType}`);
    await capturePlatformStep(host, capture, "platform-1", "clone-worker-ready", [
      ready.data.phase,
      ready.data.cloneType,
    ]);

    const cycle: { label: string; self?: unknown } = { label: meta.framework };
    cycle.self = cycle;
    const regexp = /worker-(clone)/giu;
    regexp.lastIndex = 7;
    const resultPromise = nextWorkerMessage<MessageRecord>(owner.worker, "rich clone result");
    owner.worker.postMessage({
      date: new Date("2024-02-29T12:34:56.000Z"),
      regexp,
      map: new Map<string, unknown>([
        ["alpha", spec.seed],
        ["beta", { count: spec.variant + 2 }],
      ]),
      set: new Set(["first", meta.framework, "last"]),
      cycle,
      bigint: 9007199254740993n + BigInt(spec.variant),
      typed: new Uint16Array([1, spec.seed, 65535]),
      sparse: [, "middle", undefined],
    });
    const result = await resultPromise;
    assertFixture(result.data.phase === "cloned", "worker returned the rich clone result");
    assertFixture(result.data.cycle === true, "structured clone retained the object cycle");
    assertFixture(result.data.hole === false, "structured clone retained an array hole");
    const summary = [
      result.data.date,
      result.data.regexp,
      result.data.map,
      result.data.set,
      result.data.cycle,
      result.data.bigint,
      result.data.typed,
      result.data.hole,
      result.data.undefinedValue,
    ].join("\n");
    output(root, "clone-result", summary);
    await capturePlatformStep(host, capture, "platform-2", "clone-rich-values", [
      result.data.map,
      result.data.set,
      result.data.cycle,
      result.data.typed,
    ]);

    return [
      fact("date", result.data.date),
      fact("regexp", result.data.regexp),
      fact("map", result.data.map),
      fact("set", result.data.set),
      fact("cycle", result.data.cycle),
      fact("bigint", result.data.bigint),
      fact("typed", result.data.typed),
      fact("sparse-hole", result.data.hole),
    ];
  } finally {
    disposeBlobWorker(owner);
  }
}

async function arrayBufferTransferRoundtrip(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const owner = createBlobWorker(
    `postMessage({ phase: "ready" });
addEventListener("message", (event) => {
  const buffer = event.data.buffer;
  const bytes = new Uint8Array(buffer);
  const before = Array.from(bytes);
  for (let index = 0; index < bytes.length; index += 1) bytes[index] += event.data.delta;
  const after = Array.from(bytes);
  postMessage({ phase: "returned", before, after, buffer }, [buffer]);
});`,
    `buffer-${tokenFor(meta, spec)}`,
  );
  try {
    const ready = await nextWorkerMessage<MessageRecord>(owner.worker, "buffer worker ready");
    assertFixture(ready.data.phase === "ready", "buffer worker became ready");
    const buffer = new Uint8Array([1, 3, 5, spec.variant + 7]).buffer;
    const initial = Array.from(new Uint8Array(buffer)).join("|");
    output(root, "buffer-ready", `${initial}\nbyteLength=${buffer.byteLength}`);
    await capturePlatformStep(host, capture, "platform-1", "buffer-before-transfer", [
      initial,
      buffer.byteLength,
    ]);

    const returnedPromise = nextWorkerMessage<MessageRecord>(owner.worker, "buffer round trip");
    owner.worker.postMessage({ buffer, delta: 10 }, [buffer]);
    const detachedLength = buffer.byteLength;
    assertFixture(detachedLength === 0, "transferring ArrayBuffer detached the sender buffer");
    const returned = await returnedPromise;
    const returnedBuffer = returned.data.buffer;
    assertFixture(returnedBuffer instanceof ArrayBuffer, "worker transferred an ArrayBuffer back");
    const returnedBytes = Array.from(new Uint8Array(returnedBuffer)).join("|");
    assertFixture(
      returnedBytes === (returned.data.after as number[]).join("|"),
      "returned transfer bytes matched the worker mutation",
    );
    output(
      root,
      "buffer-returned",
      `detached=${detachedLength}\nbefore=${(returned.data.before as number[]).join("|")}\nafter=${returnedBytes}`,
    );
    await capturePlatformStep(host, capture, "platform-2", "buffer-returned", [
      detachedLength,
      returnedBuffer.byteLength,
      returnedBytes,
      returned.ports.length,
    ]);

    return [
      fact("initial", initial),
      fact("sender-detached", detachedLength),
      fact("worker-before", (returned.data.before as number[]).join("|")),
      fact("returned", returnedBytes),
      fact("returned-byte-length", returnedBuffer.byteLength),
    ];
  } finally {
    disposeBlobWorker(owner);
  }
}

async function messagePortTransferPipeline(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const owner = createBlobWorker(
    `addEventListener("message", (event) => {
  const port = event.ports[0];
  port.addEventListener("message", (message) => {
    port.postMessage({ phase: "port-reply", value: message.data.value + ":worker", nested: message.data.nested.count + 1 });
  });
  port.start();
  postMessage({ phase: "attached", transferredPorts: event.ports.length, label: event.data.label });
});`,
    `port-${tokenFor(meta, spec)}`,
  );
  const channel = new MessageChannel();
  try {
    const attachedPromise = nextWorkerMessage<MessageRecord>(owner.worker, "worker port attach");
    owner.worker.postMessage({ label: meta.framework }, [channel.port2]);
    const attached = await attachedPromise;
    assertFixture(attached.data.transferredPorts === 1, "worker received one transferred port");
    output(
      root,
      "port-attached",
      `${attached.data.phase}:${attached.data.label}:ports=${attached.data.transferredPorts}`,
    );
    await capturePlatformStep(host, capture, "platform-1", "message-port-attached", [
      attached.data.phase,
      attached.data.label,
      attached.data.transferredPorts,
    ]);

    const portReplyPromise = nextPortMessage<MessageRecord>(channel.port1, "worker port reply");
    channel.port1.start();
    channel.port1.postMessage({
      value: `${meta.framework}-${spec.variant}`,
      nested: { count: spec.seed },
    });
    const reply = await portReplyPromise;
    assertFixture(reply.data.phase === "port-reply", "transferred port returned a reply");
    assertFixture(reply.target === channel.port1, "MessagePort event target was the receiving port");
    output(
      root,
      "port-reply",
      `${reply.data.value}:${reply.data.nested}:origin=${reply.origin}:ports=${reply.ports.length}`,
    );
    await capturePlatformStep(host, capture, "platform-2", "message-port-reply", [
      reply.data.value,
      reply.data.nested,
      reply.origin,
      reply.ports.length,
    ]);

    return [
      fact("transferred-ports", attached.data.transferredPorts),
      fact("reply", reply.data.value),
      fact("nested", reply.data.nested),
      fact("origin", reply.origin),
      fact("target-port", reply.target === channel.port1),
    ];
  } finally {
    channel.port1.close();
    disposeBlobWorker(owner);
  }
}

async function classicImportScriptsStack(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const name = `classic-${tokenFor(meta, spec)}`;
  const worker = new Worker(
    `/support/worker-classic-import.js?token=${encodeURIComponent(tokenFor(meta, spec))}`,
    { name },
  );
  try {
    const loaded = await nextWorkerMessage<MessageRecord>(worker, "classic imports loaded");
    const sequence = (loaded.data.sequence as string[]).join("|");
    assertFixture(sequence === "a|b:6", "importScripts evaluated dependencies in argument order");
    assertFixture(loaded.data.name === name, "classic worker retained its name");
    output(
      root,
      "classic-loaded",
      `${sequence}\n${loaded.data.path}\n${loaded.data.importScriptsType}\n${loaded.data.name}`,
    );
    await capturePlatformStep(host, capture, "platform-1", "classic-imports-loaded", [
      sequence,
      loaded.data.path,
      loaded.data.importScriptsType,
    ]);

    const computedPromise = nextWorkerMessage<MessageRecord>(worker, "classic helper computation");
    worker.postMessage({ value: spec.variant + 4 });
    const computed = await computedPromise;
    assertFixture(computed.data.phase === "computed", "classic worker used imported helpers");
    output(
      root,
      "classic-computed",
      `${computed.data.input}:${computed.data.doubled}:${computed.data.tripled}`,
    );
    await capturePlatformStep(host, capture, "platform-2", "classic-imports-computed", [
      computed.data.input,
      computed.data.doubled,
      computed.data.tripled,
      (computed.data.sequence as string[]).join("|"),
    ]);

    return [
      fact("sequence", sequence),
      fact("path", loaded.data.path),
      fact("import-scripts", loaded.data.importScriptsType),
      fact("doubled", computed.data.doubled),
      fact("tripled", computed.data.tripled),
    ];
  } finally {
    worker.terminate();
  }
}

async function moduleWorkerGraph(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const name = `module-${tokenFor(meta, spec)}`;
  const worker = new Worker(
    `/support/worker-module-entry.js?token=${encodeURIComponent(tokenFor(meta, spec))}`,
    { type: "module", name },
  );
  try {
    const loaded = await nextWorkerMessage<MessageRecord>(worker, "module worker loaded");
    assertFixture(loaded.data.phase === "module-loaded", "module worker evaluated its static import");
    assertFixture(loaded.data.base === 17, "module dependency exported its constant");
    output(
      root,
      "module-loaded",
      `${loaded.data.description}\n${loaded.data.path}\n${loaded.data.importScriptsType}\n${loaded.data.name}`,
    );
    await capturePlatformStep(host, capture, "platform-1", "module-static-import", [
      loaded.data.base,
      loaded.data.description,
      loaded.data.importScriptsType,
      loaded.data.path,
    ]);

    const dynamicPromise = nextWorkerMessage<MessageRecord>(worker, "module dynamic import");
    worker.postMessage({ value: spec.variant + 5 });
    const dynamic = await dynamicPromise;
    assertFixture(dynamic.data.sameBase === true, "dynamic import resolved the same module export");
    const keys = (dynamic.data.keys as string[]).join("|");
    output(root, "module-dynamic", `${dynamic.data.description}\n${keys}`);
    await capturePlatformStep(host, capture, "platform-2", "module-dynamic-import", [
      dynamic.data.sameBase,
      dynamic.data.description,
      keys,
    ]);

    return [
      fact("static", loaded.data.description),
      fact("dynamic", dynamic.data.description),
      fact("module-keys", keys),
      fact("same-base", dynamic.data.sameBase),
      fact("import-scripts-type", loaded.data.importScriptsType),
    ];
  } finally {
    worker.terminate();
  }
}

async function workerFetchStream(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = tokenFor(meta, spec);
  const owner = createBlobWorker(
    `postMessage({ phase: "fetch-ready", fetchType: typeof fetch, streamType: typeof ReadableStream });
addEventListener("message", async (event) => {
  try {
    const response = await fetch(event.data.url);
    const clone = response.clone();
    const [left, right] = response.body.tee();
    const [leftText, rightText, clonePayload] = await Promise.all([
      new Response(left).text(),
      new Response(right).text(),
      clone.json(),
    ]);
    const payload = JSON.parse(leftText);
    postMessage({
      phase: "fetch-complete",
      status: response.status,
      contentType: response.headers.get("content-type"),
      bodyUsed: response.bodyUsed,
      branchesEqual: leftText === rightText,
      cloneEqual: JSON.stringify(payload) === JSON.stringify(clonePayload),
      token: payload.token,
      items: payload.items,
      text: payload.text,
      bytes: new TextEncoder().encode(leftText).byteLength,
    });
  } catch (error) {
    postMessage({ phase: "fetch-error", name: error?.name ?? "Error", message: String(error?.message ?? error) });
  }
});`,
    `fetch-${token}`,
  );
  try {
    const ready = await nextWorkerMessage<MessageRecord>(owner.worker, "worker fetch ready");
    assertFixture(ready.data.fetchType === "function", "worker exposed fetch");
    assertFixture(ready.data.streamType === "function", "worker exposed ReadableStream");
    output(root, "fetch-ready", `${ready.data.fetchType}:${ready.data.streamType}`);
    await capturePlatformStep(host, capture, "platform-1", "worker-fetch-ready", [
      ready.data.fetchType,
      ready.data.streamType,
    ]);

    const completePromise = nextWorkerMessage<MessageRecord>(owner.worker, "worker fetch stream");
    owner.worker.postMessage({
      url: new URL(
        `/support/network/stream-payload?token=${encodeURIComponent(token)}`,
        location.href,
      ).href,
    });
    const complete = await completePromise;
    assertFixture(
      complete.data.phase === "fetch-complete",
      `worker fetch completed instead of ${String(complete.data.name ?? complete.data.phase)}`,
    );
    assertFixture(complete.data.status === 200, "worker fetch returned HTTP 200");
    assertFixture(complete.data.branchesEqual === true, "worker stream tee branches matched");
    assertFixture(complete.data.cloneEqual === true, "worker response clone matched stream bytes");
    assertFixture(complete.data.token === token, "worker response retained the request token");
    const items = (complete.data.items as string[]).join("|");
    output(
      root,
      "fetch-complete",
      `${complete.data.status}:${complete.data.contentType}\n${items}\n${complete.data.text}\nbytes=${complete.data.bytes}`,
    );
    await capturePlatformStep(host, capture, "platform-2", "worker-fetch-consumed", [
      complete.data.status,
      complete.data.bodyUsed,
      complete.data.branchesEqual,
      complete.data.cloneEqual,
      items,
      complete.data.bytes,
    ]);

    return [
      fact("status", complete.data.status),
      fact("content-type", complete.data.contentType),
      fact("body-used", complete.data.bodyUsed),
      fact("branches-equal", complete.data.branchesEqual),
      fact("clone-equal", complete.data.cloneEqual),
      fact("items", items),
      fact("unicode", complete.data.text),
      fact("bytes", complete.data.bytes),
    ];
  } finally {
    disposeBlobWorker(owner);
  }
}

async function sharedWorkerMultiPort(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = tokenFor(meta, spec);
  const url = `/support/shared-worker-multi.js?token=${encodeURIComponent(token)}`;
  const name = `shared-${token}`;
  const first = new SharedWorker(url, { name });
  const firstConnectedPromise = nextPortMessage<MessageRecord>(
    first.port,
    "first shared worker connection",
  );
  first.port.start();
  const firstConnected = await firstConnectedPromise;
  const firstCount = await nextPortMessage<MessageRecord>(first.port, "first shared worker count");
  assertFixture(firstConnected.data.index === 1, "first SharedWorker port received index one");
  assertFixture(firstCount.data.count === 1, "first SharedWorker port observed one client");
  output(
    root,
    "shared-first",
    `${firstConnected.data.phase}:${firstConnected.data.index}:${firstConnected.data.name}\nclients=${firstCount.data.count}`,
  );
  await capturePlatformStep(host, capture, "platform-1", "shared-worker-first-client", [
    firstConnected.data.index,
    firstConnected.data.name,
    firstCount.data.count,
  ]);

  const firstCountTwoPromise = nextPortMessage<MessageRecord>(
    first.port,
    "first shared worker second count",
  );
  const second = new SharedWorker(url, name);
  const secondConnectedPromise = nextPortMessage<MessageRecord>(
    second.port,
    "second shared worker connection",
  );
  second.port.start();
  const secondConnected = await secondConnectedPromise;
  const secondCountPromise = nextPortMessage<MessageRecord>(
    second.port,
    "second shared worker count",
  );
  const [firstCountTwo, secondCount] = await Promise.all([
    firstCountTwoPromise,
    secondCountPromise,
  ]);
  assertFixture(secondConnected.data.index === 2, "second SharedWorker port joined same instance");
  assertFixture(
    firstCountTwo.data.count === 2 && secondCount.data.count === 2,
    "both SharedWorker clients observed the shared client count",
  );
  const pongPromise = nextPortMessage<MessageRecord>(second.port, "shared worker pong");
  second.port.postMessage({ type: "ping", value: `${meta.framework}-${spec.variant}` });
  const pong = await pongPromise;
  assertFixture(pong.data.index === 2, "SharedWorker reply retained the sending client index");
  output(
    root,
    "shared-second",
    `${secondConnected.data.phase}:${secondConnected.data.index}\ncounts=${firstCountTwo.data.count}|${secondCount.data.count}\npong=${pong.data.value}:${pong.data.clients}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "shared-worker-second-client", [
    secondConnected.data.index,
    firstCountTwo.data.count,
    secondCount.data.count,
    pong.data.value,
    pong.data.clients,
  ]);

  const firstShutdownPromise = nextPortMessage<MessageRecord>(first.port, "first shared shutdown");
  const secondShutdownPromise = nextPortMessage<MessageRecord>(
    second.port,
    "second shared shutdown",
  );
  second.port.postMessage({ type: "shutdown" });
  const [firstShutdown, secondShutdown] = await Promise.all([
    firstShutdownPromise,
    secondShutdownPromise,
  ]);
  first.port.close();
  second.port.close();

  return [
    fact("first-index", firstConnected.data.index),
    fact("second-index", secondConnected.data.index),
    fact("client-counts", `${firstCount.data.count}|${firstCountTwo.data.count}`),
    fact("pong", pong.data.value),
    fact("pong-clients", pong.data.clients),
    fact(
      "shutdown-clients",
      `${firstShutdown.data.clients}|${secondShutdown.data.clients}`,
    ),
  ];
}

async function broadcastChannelBridge(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = tokenFor(meta, spec);
  const channelName = `worker-bridge-${token}`;
  const receiver = new BroadcastChannel(channelName);
  const sender = new BroadcastChannel(channelName);
  const owner = createBlobWorker(
    `let channel;
addEventListener("message", (event) => {
  if (event.data.type === "init") {
    channel = new BroadcastChannel(event.data.name);
    channel.addEventListener("message", (message) => {
      channel.postMessage({ kind: "worker-reply", value: message.data.value + ":worker" });
      postMessage({ phase: "handled", value: message.data.value, origin: message.origin });
    });
    postMessage({ phase: "bridge-ready", channel: channel.name });
    return;
  }
  if (event.data.type === "close") {
    channel.close();
    postMessage({ phase: "closed" });
    close();
  }
});`,
    `broadcast-${token}`,
  );
  try {
    const readyPromise = nextWorkerMessage<MessageRecord>(owner.worker, "worker channel ready");
    owner.worker.postMessage({ type: "init", name: channelName });
    const ready = await readyPromise;
    assertFixture(ready.data.channel === channelName, "worker opened the requested BroadcastChannel");
    output(root, "broadcast-ready", `${ready.data.phase}:${ready.data.channel}`);
    await capturePlatformStep(host, capture, "platform-1", "worker-broadcast-ready", [
      ready.data.phase,
      ready.data.channel,
      receiver.name,
      sender.name,
    ]);

    const handledPromise = nextWorkerMessage<MessageRecord>(owner.worker, "worker channel handled");
    const requestSeenPromise = nextBroadcastMessage<MessageRecord>(
      receiver,
      "window channel request",
    );
    sender.postMessage({ value: `${meta.framework}-${spec.variant}` });
    const requestSeen = await requestSeenPromise;
    const replyPromise = nextBroadcastMessage<MessageRecord>(receiver, "window channel reply");
    const [handled, reply] = await Promise.all([handledPromise, replyPromise]);
    assertFixture(
      requestSeen.data.value === `${meta.framework}-${spec.variant}`,
      "window receiver observed the original window broadcast first",
    );
    assertFixture(handled.data.phase === "handled", "worker observed the window broadcast");
    assertFixture(reply.data.kind === "worker-reply", "window observed the worker broadcast");
    assertFixture(
      handled.data.origin === location.origin && reply.origin === location.origin,
      "both BroadcastChannel deliveries exposed the page origin",
    );
    const workerOrigin = stableOrigin(String(handled.data.origin));
    const windowOrigin = stableOrigin(reply.origin);
    output(
      root,
      "broadcast-reply",
      `${requestSeen.data.value}\n${handled.data.value}\n${reply.data.value}\nworker-origin=${workerOrigin}\nwindow-origin=${windowOrigin}`,
    );
    await capturePlatformStep(host, capture, "platform-2", "worker-broadcast-roundtrip", [
      handled.data.value,
      requestSeen.data.value,
      reply.data.value,
      workerOrigin,
      windowOrigin,
      reply.source === null,
    ]);

    const closedPromise = nextWorkerMessage<MessageRecord>(owner.worker, "worker channel close");
    owner.worker.postMessage({ type: "close" });
    const closed = await closedPromise;
    assertFixture(closed.data.phase === "closed", "worker closed its BroadcastChannel");

    return [
      fact("channel", channelName),
      fact("request", requestSeen.data.value),
      fact("handled", handled.data.value),
      fact("reply", reply.data.value),
      fact("worker-origin", workerOrigin),
      fact("window-origin", windowOrigin),
      fact("source-null", reply.source === null),
      fact("closed", closed.data.phase),
    ];
  } finally {
    receiver.close();
    sender.close();
    disposeBlobWorker(owner);
  }
}

async function terminateAndReplace(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const source = `postMessage({ phase: "boot", name: self.name });
addEventListener("message", (event) => postMessage({ phase: "echo", value: event.data.value, name: self.name }));`;
  const url = URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
  const name = `replacement-${tokenFor(meta, spec)}`;
  const first = new Worker(url, { name });
  let second: Worker | null = null;
  try {
    const firstBoot = await nextWorkerMessage<MessageRecord>(first, "first replacement worker boot");
    const firstEchoPromise = nextWorkerMessage<MessageRecord>(first, "first replacement worker echo");
    first.postMessage({ value: `first-${spec.variant}` });
    const firstEcho = await firstEchoPromise;
    assertFixture(firstBoot.data.name === name, "first worker retained its name");
    output(
      root,
      "termination-first",
      `${firstBoot.data.phase}:${firstBoot.data.name}\n${firstEcho.data.phase}:${firstEcho.data.value}`,
    );
    await capturePlatformStep(host, capture, "platform-1", "worker-before-terminate", [
      firstBoot.data.phase,
      firstEcho.data.value,
      firstEcho.data.name,
    ]);

    const terminateResult = first.terminate();
    let postAfterTerminate = "no-throw";
    try {
      first.postMessage({ value: "ignored" });
    } catch (error: unknown) {
      postAfterTerminate = error instanceof Error ? error.name : "unknown";
    }
    second = new Worker(url, { name });
    const secondBoot = await nextWorkerMessage<MessageRecord>(second, "second replacement worker boot");
    const secondEchoPromise = nextWorkerMessage<MessageRecord>(
      second,
      "second replacement worker echo",
    );
    second.postMessage({ value: `second-${spec.seed}` });
    const secondEcho = await secondEchoPromise;
    assertFixture(secondBoot.data.name === name, "replacement worker retained the same name");
    assertFixture(secondEcho.data.value === `second-${spec.seed}`, "replacement worker echoed input");
    output(
      root,
      "termination-second",
      `terminate=${String(terminateResult)}\npost=${postAfterTerminate}\n${secondBoot.data.phase}:${secondEcho.data.value}`,
    );
    await capturePlatformStep(host, capture, "platform-2", "worker-replacement-ready", [
      String(terminateResult),
      postAfterTerminate,
      secondBoot.data.phase,
      secondEcho.data.value,
      secondEcho.data.name,
    ]);

    return [
      fact("first", firstEcho.data.value),
      fact("terminate-return", String(terminateResult)),
      fact("post-after-terminate", postAfterTerminate),
      fact("second", secondEcho.data.value),
      fact("same-name", firstBoot.data.name === secondBoot.data.name),
    ];
  } finally {
    first.terminate();
    second?.terminate();
    URL.revokeObjectURL(url);
  }
}

const SCENARIOS: Record<string, WorkerScenario> = {
  "dedicated-event-order": dedicatedEventOrder,
  "structured-clone-rich-values": structuredCloneRichValues,
  "arraybuffer-transfer-roundtrip": arrayBufferTransferRoundtrip,
  "messageport-transfer-pipeline": messagePortTransferPipeline,
  "classic-importscripts-stack": classicImportScriptsStack,
  "module-worker-graph": moduleWorkerGraph,
  "worker-fetch-stream": workerFetchStream,
  "shared-worker-multi-port": sharedWorkerMultiPort,
  "broadcast-channel-bridge": broadcastChannelBridge,
  "terminate-and-replace": terminateAndReplace,
};

export async function runWorkerMessagingBoundaryCase(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformBoundaryResult> {
  const scenario = SCENARIOS[spec.slug];
  assertFixture(scenario, `missing worker messaging scenario ${spec.slug}`);
  const facts = await scenario(host, meta, spec, capture);
  return { status: "ready", facts };
}

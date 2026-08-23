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

type ServiceWorkerScenario = (
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
) => Promise<PlatformFact[]>;

interface WorkerReply {
  command: string;
  token?: string;
  version?: string;
  clientCount?: number;
  clientPaths?: string[];
  scopePath?: string;
  buffer?: ArrayBuffer;
  byteLength?: number;
  key?: string;
  cacheName?: string;
}

interface SyntheticPayload {
  token: string;
  version: string;
  method: string;
  mode: string;
  credentials: string;
  destination: string;
  header: string;
  body: string;
}

function scenarioRoot(host: HTMLElement, name: string): HTMLElement {
  const root = document.createElement("section");
  root.dataset.serviceWorkerScenario = name;
  host.append(root);
  return root;
}

function output(root: HTMLElement, name: string, value: string): HTMLOutputElement {
  const element = document.createElement("output");
  element.dataset.serviceWorkerOutput = name;
  element.textContent = value;
  root.append(element);
  return element;
}

function tokenFor(meta: SmokeMeta, spec: CaseSpec): string {
  return `${meta.framework}-${spec.seed}-${spec.variant}`;
}

function workerScriptUrl(token: string, version = "v1"): string {
  return `/support/service-worker/worker.js?token=${encodeURIComponent(token)}&version=${encodeURIComponent(version)}`;
}

function urlProjection(value: string): string {
  const url = new URL(value);
  return `${url.pathname}${url.search}`;
}

async function waitForActivated(worker: ServiceWorker): Promise<void> {
  if (worker.state === "activated") {
    return;
  }
  await withEventTimeout(
    new Promise<void>((resolve, reject) => {
      const onStateChange = (): void => {
        if (worker.state === "activated") {
          worker.removeEventListener("statechange", onStateChange);
          resolve();
        } else if (worker.state === "redundant") {
          worker.removeEventListener("statechange", onStateChange);
          reject(new Error("service worker became redundant before activation"));
        }
      };
      worker.addEventListener("statechange", onStateChange);
      onStateChange();
    }),
    "service worker activated state",
  );
}

async function controllerForRegistration(
  registration: ServiceWorkerRegistration,
): Promise<ServiceWorker> {
  const ready = await withEventTimeout(
    navigator.serviceWorker.ready,
    "service worker ready registration",
  );
  assertFixture(ready.scope === registration.scope, "ready resolved the requested registration");
  const active = ready.active;
  assertFixture(active, "ready registration exposed an active worker");
  await waitForActivated(active);
  if (navigator.serviceWorker.controller === null) {
    await withEventTimeout(
      new Promise<void>((resolve) => {
        navigator.serviceWorker.addEventListener("controllerchange", () => resolve(), {
          once: true,
        });
      }),
      "service worker controllerchange",
    );
  }
  const controller = navigator.serviceWorker.controller;
  assertFixture(controller, "active service worker claimed the current page");
  await waitForActivated(controller);
  return controller;
}

async function registerWorker(
  token: string,
  version = "v1",
): Promise<{ registration: ServiceWorkerRegistration; controller: ServiceWorker }> {
  const registration = await navigator.serviceWorker.register(
    workerScriptUrl(token, version),
    { scope: "/", updateViaCache: "none" },
  );
  const controller = await controllerForRegistration(registration);
  return { registration, controller };
}

async function cleanupRegistration(registration: ServiceWorkerRegistration): Promise<void> {
  await registration.unregister();
  for (const cacheName of await caches.keys()) {
    if (cacheName.startsWith("frontend-smoke-sw-")) {
      await caches.delete(cacheName);
    }
  }
}

function nextPortMessage(port: MessagePort, label: string): Promise<MessageEvent<WorkerReply>> {
  return withEventTimeout(
    new Promise<MessageEvent<WorkerReply>>((resolve, reject) => {
      const cleanup = (): void => {
        port.removeEventListener("message", onMessage as EventListener);
        port.removeEventListener("messageerror", onMessageError as EventListener);
      };
      const onMessage = (event: MessageEvent<WorkerReply>): void => {
        cleanup();
        resolve(event);
      };
      const onMessageError = (): void => {
        cleanup();
        reject(new Error(`${label} emitted messageerror`));
      };
      port.addEventListener("message", onMessage as EventListener);
      port.addEventListener("messageerror", onMessageError as EventListener);
      port.start();
    }),
    label,
  );
}

async function sendWorkerCommand(
  worker: ServiceWorker,
  data: Record<string, unknown>,
  transfer: Transferable[] = [],
): Promise<WorkerReply> {
  const channel = new MessageChannel();
  const reply = nextPortMessage(channel.port1, `service worker ${String(data.command)} reply`);
  worker.postMessage(data, [channel.port2, ...transfer]);
  const event = await reply;
  channel.port1.close();
  return event.data;
}

async function registrationReadyController(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = tokenFor(meta, spec);
  const { registration, controller } = await registerWorker(token);
  try {
    const active = registration.active;
    assertFixture(active?.state === "activated", "registration exposed an activated worker");
    assertFixture(controller.state === "activated", "controller exposed activated state");
    assertFixture(active === controller, "registration and container shared worker identity");
    output(
      root,
      "registration-ready",
      `${new URL(registration.scope).pathname}\n${urlProjection(active.scriptURL)}\n${controller.state}`,
    );
    await capturePlatformStep(host, capture, "platform-1", "service-worker-ready", [
      new URL(registration.scope).pathname,
      urlProjection(active.scriptURL),
      controller.state,
      active === controller,
    ]);

    const byDocument = await navigator.serviceWorker.getRegistration(location.href);
    const registrations = await navigator.serviceWorker.getRegistrations();
    assertFixture(byDocument === registration, "getRegistration retained registration identity");
    assertFixture(registrations.includes(registration), "getRegistrations included active scope");
    output(
      root,
      "registration-query",
      `${byDocument === registration}\n${registrations.length}\n${registrations.map((item) => new URL(item.scope).pathname).join("|")}`,
    );
    await capturePlatformStep(host, capture, "platform-2", "service-worker-registry-query", [
      byDocument === registration,
      registrations.length,
      registrations.map((item) => new URL(item.scope).pathname).join(","),
    ]);

    return [
      fact("scope", new URL(registration.scope).pathname),
      fact("script", urlProjection(active.scriptURL)),
      fact("active-state", active.state),
      fact("controller-state", controller.state),
      fact("shared-worker-identity", active === controller),
      fact("registry-count", registrations.length),
    ];
  } finally {
    await cleanupRegistration(registration);
  }
}

async function syntheticFetchMetadata(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = tokenFor(meta, spec);
  const { registration } = await registerWorker(token);
  try {
    const request = new Request("/support/service-worker/synthetic", {
      method: "POST",
      credentials: "include",
      headers: {
        "Content-Type": "text/plain;charset=utf-8",
        "X-Smoke-Token": token,
      },
      body: `${meta.framework}:payload:${spec.variant}`,
    });
    output(root, "synthetic-request", `${request.method}\n${request.mode}\n${request.credentials}`);
    await capturePlatformStep(host, capture, "platform-1", "service-worker-fetch-request", [
      request.method,
      request.mode,
      request.credentials,
      request.headers.get("x-smoke-token"),
    ]);

    const response = await fetch(request);
    const payload = (await response.json()) as SyntheticPayload;
    assertFixture(response.status === 201, "synthetic response retained status");
    assertFixture(payload.token === token, "fetch event retained case token");
    assertFixture(payload.method === "POST", "fetch event retained POST method");
    assertFixture(payload.header === token, "fetch event retained custom request header");
    assertFixture(payload.body === `${meta.framework}:payload:${spec.variant}`, "fetch event cloned body");
    output(
      root,
      "synthetic-response",
      `${response.status}:${response.statusText}\n${response.type}\n${response.headers.get("x-service-worker")}\n${JSON.stringify(payload)}`,
    );
    await capturePlatformStep(host, capture, "platform-2", "service-worker-fetch-response", [
      response.status,
      response.statusText,
      response.type,
      response.headers.get("x-service-worker"),
      JSON.stringify(payload),
    ]);

    return [
      fact("status", response.status),
      fact("type", response.type),
      fact("method", payload.method),
      fact("metadata", `${payload.mode}|${payload.credentials}|${payload.destination}`),
      fact("body", payload.body),
    ];
  } finally {
    await cleanupRegistration(registration);
  }
}

async function precacheFetchRoundtrip(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = tokenFor(meta, spec);
  const { registration, controller } = await registerWorker(token);
  try {
    const response = await fetch(`/support/service-worker/cache-probe?case=${spec.variant}`);
    const text = await response.text();
    assertFixture(text === `precache:${token}:v1`, "fetch handler returned install precache entry");
    output(root, "precache-fetch", `${response.status}\n${response.headers.get("x-sw-version")}\n${text}`);
    await capturePlatformStep(host, capture, "platform-1", "service-worker-precache-fetch", [
      response.status,
      response.headers.get("x-sw-version"),
      text,
    ]);

    const value = `message-cache:${meta.framework}:${spec.seed}`;
    const reply = await sendWorkerCommand(controller, { command: "cache-write", value });
    assertFixture(reply.command === "cache-write", "worker acknowledged message cache write");
    const cached = await caches.match(reply.key ?? "missing");
    assertFixture(cached, "window CacheStorage observed worker cache write");
    const cachedText = await cached.text();
    assertFixture(cachedText === value, "shared cache retained message value");
    output(
      root,
      "message-cache",
      `${reply.cacheName}\n${reply.key}\n${cached.headers.get("x-message-cache")}\n${cachedText}`,
    );
    await capturePlatformStep(host, capture, "platform-2", "service-worker-shared-cache", [
      reply.cacheName,
      reply.key,
      cached.headers.get("x-message-cache"),
      cachedText,
    ]);

    return [
      fact("precache", text),
      fact("precache-version", response.headers.get("x-sw-version")),
      fact("message-cache", cachedText),
      fact("cache-name", reply.cacheName),
      fact("cache-key", reply.key),
    ];
  } finally {
    await cleanupRegistration(registration);
  }
}

async function cacheStorageLifecycle(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = tokenFor(meta, spec);
  const { registration, controller } = await registerWorker(token);
  try {
    const reply = await sendWorkerCommand(controller, {
      command: "cache-write",
      value: `cache-lifecycle:${meta.framework}:${spec.variant}`,
    });
    const namesBefore = await caches.keys();
    const hasBefore = await caches.has(reply.cacheName ?? "missing");
    const cache = await caches.open(reply.cacheName ?? "missing");
    const keys = await cache.keys();
    assertFixture(namesBefore.includes(reply.cacheName ?? ""), "CacheStorage listed worker cache");
    assertFixture(hasBefore, "CacheStorage.has observed worker cache");
    assertFixture(keys.length >= 2, "worker cache retained precache and message entries");
    output(
      root,
      "cache-before-delete",
      `${namesBefore.join("|")}\n${hasBefore}\n${keys.map((request) => new URL(request.url).pathname).sort().join("|")}`,
    );
    await capturePlatformStep(host, capture, "platform-1", "service-worker-cache-keys", [
      namesBefore.join(","),
      hasBefore,
      keys.length,
      keys.map((request) => new URL(request.url).pathname).sort().join(","),
    ]);

    const deleted = await caches.delete(reply.cacheName ?? "missing");
    const deletedAgain = await caches.delete(reply.cacheName ?? "missing");
    const hasAfter = await caches.has(reply.cacheName ?? "missing");
    const namesAfter = await caches.keys();
    assertFixture(deleted && !deletedAgain, "CacheStorage delete reflected cache lifecycle");
    assertFixture(!hasAfter, "CacheStorage.has reflected deletion");
    assertFixture(!namesAfter.includes(reply.cacheName ?? ""), "deleted cache disappeared from keys");
    output(root, "cache-after-delete", `${deleted}:${deletedAgain}\n${hasAfter}\n${namesAfter.join("|")}`);
    await capturePlatformStep(host, capture, "platform-2", "service-worker-cache-delete", [
      deleted,
      deletedAgain,
      hasAfter,
      namesAfter.join(","),
    ]);

    return [
      fact("names-before", namesBefore.join("|")),
      fact("key-count", keys.length),
      fact("has-before", hasBefore),
      fact("deleted", deleted),
      fact("deleted-again", deletedAgain),
      fact("has-after", hasAfter),
      fact("names-after", namesAfter.join("|")),
    ];
  } finally {
    await cleanupRegistration(registration);
  }
}

async function messageChannelBinaryTransfer(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = tokenFor(meta, spec);
  const { registration, controller } = await registerWorker(token);
  try {
    const buffer = new Uint8Array([spec.variant, 3, 7, 127, 255]).buffer;
    output(root, "transfer-before", `${buffer.byteLength}\n${Array.from(new Uint8Array(buffer)).join("|")}`);
    await capturePlatformStep(host, capture, "platform-1", "service-worker-transfer-before", [
      buffer.byteLength,
      Array.from(new Uint8Array(buffer)).join(","),
    ]);

    const reply = await sendWorkerCommand(
      controller,
      { command: "transfer", buffer },
      [buffer],
    );
    assertFixture(buffer.byteLength === 0, "postMessage detached source ArrayBuffer");
    assertFixture(reply.buffer instanceof ArrayBuffer, "worker returned transferred ArrayBuffer");
    const returned = Array.from(new Uint8Array(reply.buffer));
    assertFixture(returned.join("|") === `255|127|7|3|${spec.variant}`, "worker reversed transferred bytes");
    output(root, "transfer-after", `${buffer.byteLength}\n${reply.byteLength}\n${returned.join("|")}`);
    await capturePlatformStep(host, capture, "platform-2", "service-worker-transfer-after", [
      buffer.byteLength,
      reply.byteLength,
      returned.join(","),
    ]);

    return [
      fact("source-length", buffer.byteLength),
      fact("returned-length", reply.byteLength),
      fact("returned-bytes", returned.join("|")),
      fact("returned-brand", reply.buffer instanceof ArrayBuffer),
    ];
  } finally {
    await cleanupRegistration(registration);
  }
}

async function clientsClaimInspection(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = tokenFor(meta, spec);
  const { registration, controller } = await registerWorker(token);
  try {
    output(root, "client-controller", `${controller.state}\n${urlProjection(controller.scriptURL)}`);
    await capturePlatformStep(host, capture, "platform-1", "service-worker-client-controlled", [
      controller.state,
      urlProjection(controller.scriptURL),
      navigator.serviceWorker.controller === controller,
    ]);

    const reply = await sendWorkerCommand(controller, { command: "inspect" });
    assertFixture(reply.command === "inspect", "worker returned inspect response");
    assertFixture(reply.clientCount === 1, "worker observed exactly the current isolated window client");
    assertFixture(reply.clientPaths?.[0] === location.pathname, "worker client URL matched current case path");
    assertFixture(reply.scopePath === "/", "worker registration scope covered root");
    output(
      root,
      "client-inspection",
      `${reply.clientCount}\n${reply.clientPaths?.join("|")}\n${reply.scopePath}\n${reply.version}`,
    );
    await capturePlatformStep(host, capture, "platform-2", "service-worker-clients-match-all", [
      reply.clientCount,
      reply.clientPaths?.join(","),
      reply.scopePath,
      reply.version,
    ]);

    return [
      fact("client-count", reply.clientCount),
      fact("client-paths", reply.clientPaths?.join("|")),
      fact("scope-path", reply.scopePath),
      fact("version", reply.version),
    ];
  } finally {
    await cleanupRegistration(registration);
  }
}

async function streamedResponseClone(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = tokenFor(meta, spec);
  const { registration } = await registerWorker(token);
  try {
    const response = await fetch("/support/service-worker/stream");
    const clone = response.clone();
    assertFixture(response.body && clone.body, "service worker stream exposed cloneable bodies");
    output(root, "stream-response", `${response.status}\n${response.bodyUsed}:${clone.bodyUsed}`);
    await capturePlatformStep(host, capture, "platform-1", "service-worker-stream-open", [
      response.status,
      response.bodyUsed,
      clone.bodyUsed,
      response.headers.get("content-type"),
    ]);

    const [text, bytes] = await Promise.all([response.text(), clone.arrayBuffer()]);
    const decoded = new TextDecoder().decode(bytes);
    assertFixture(text === `stream:${token}:v1:café:東京`, "stream chunks preserved Unicode order");
    assertFixture(decoded === text, "response clone retained identical bytes");
    output(root, "stream-consumed", `${text}\n${bytes.byteLength}\n${response.bodyUsed}:${clone.bodyUsed}`);
    await capturePlatformStep(host, capture, "platform-2", "service-worker-stream-consumed", [
      text,
      bytes.byteLength,
      response.bodyUsed,
      clone.bodyUsed,
    ]);

    return [
      fact("text", text),
      fact("bytes", bytes.byteLength),
      fact("clone-equal", decoded === text),
      fact("body-used", `${response.bodyUsed}:${clone.bodyUsed}`),
    ];
  } finally {
    await cleanupRegistration(registration);
  }
}

async function fallbackAndRedirect(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = tokenFor(meta, spec);
  const { registration } = await registerWorker(token);
  try {
    const fallback = await fetch(
      `/support/service-worker/fallback?token=${encodeURIComponent(token)}`,
    );
    const fallbackText = await fallback.text();
    assertFixture(fallbackText === `network-fallback:${token}`, "missing respondWith fell back to network");
    output(
      root,
      "fallback-network",
      `${fallback.status}\n${fallback.headers.get("x-network-source")}\n${fallbackText}`,
    );
    await capturePlatformStep(host, capture, "platform-1", "service-worker-network-fallback", [
      fallback.status,
      fallback.headers.get("x-network-source"),
      fallbackText,
    ]);

    const redirected = await fetch("/support/service-worker/redirect");
    const redirectedText = await redirected.text();
    assertFixture(redirected.redirected, "synthetic redirect exposed redirected state");
    assertFixture(new URL(redirected.url).pathname === "/support/service-worker/network", "redirect reached network target");
    assertFixture(redirectedText === `network-network:${token}`, "redirect target returned network body");
    output(
      root,
      "synthetic-redirect",
      `${redirected.status}\n${redirected.redirected}\n${new URL(redirected.url).pathname}\n${redirectedText}`,
    );
    await capturePlatformStep(host, capture, "platform-2", "service-worker-synthetic-redirect", [
      redirected.status,
      redirected.redirected,
      new URL(redirected.url).pathname,
      redirectedText,
    ]);

    return [
      fact("fallback", fallbackText),
      fact("fallback-source", fallback.headers.get("x-network-source")),
      fact("redirected", redirected.redirected),
      fact("redirect-path", new URL(redirected.url).pathname),
      fact("redirect-body", redirectedText),
    ];
  } finally {
    await cleanupRegistration(registration);
  }
}

async function unregisterControlledClient(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = tokenFor(meta, spec);
  const { registration, controller } = await registerWorker(token);
  const before = await navigator.serviceWorker.getRegistration(location.href);
  const registrationsBefore = await navigator.serviceWorker.getRegistrations();
  assertFixture(before === registration, "registration existed before unregister");
  output(root, "unregister-before", `${registrationsBefore.length}\n${controller.state}`);
  await capturePlatformStep(host, capture, "platform-1", "service-worker-unregister-before", [
    registrationsBefore.length,
    controller.state,
  ]);

  const first = await registration.unregister();
  const second = await registration.unregister();
  const after = await navigator.serviceWorker.getRegistration(location.href);
  const registrationsAfter = await navigator.serviceWorker.getRegistrations();
  const controlledAfter = navigator.serviceWorker.controller === controller;
  const response = await fetch("/support/service-worker/version");
  const text = await response.text();
  assertFixture(first && !second, "unregister returned true then false");
  assertFixture(after === undefined && registrationsAfter.length === 0, "registry query removed registration");
  assertFixture(controlledAfter, "existing client retained its controller after unregister");
  assertFixture(text === `version:v1:${token}`, "unregistered active worker still served controlled client");
  output(
    root,
    "unregister-after",
    `${first}:${second}\n${after === undefined}\n${registrationsAfter.length}\n${controlledAfter}\n${text}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "service-worker-unregister-after", [
    first,
    second,
    after === undefined,
    registrationsAfter.length,
    controlledAfter,
    text,
  ]);

  for (const cacheName of await caches.keys()) {
    if (cacheName.startsWith("frontend-smoke-sw-")) {
      await caches.delete(cacheName);
    }
  }
  return [
    fact("first-unregister", first),
    fact("second-unregister", second),
    fact("registry-empty", after === undefined && registrationsAfter.length === 0),
    fact("controller-retained", controlledAfter),
    fact("post-unregister-fetch", text),
  ];
}

async function updatefoundControllerReplacement(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = tokenFor(meta, spec);
  const { registration, controller: firstController } = await registerWorker(token, "v1");
  try {
    const firstResponse = await fetch("/support/service-worker/version");
    const firstText = await firstResponse.text();
    assertFixture(firstText === `version:v1:${token}`, "first active worker served v1");
    output(root, "worker-version-first", `${urlProjection(firstController.scriptURL)}\n${firstText}`);
    await capturePlatformStep(host, capture, "platform-1", "service-worker-version-one", [
      urlProjection(firstController.scriptURL),
      firstController.state,
      firstText,
    ]);

    const lifecycle: string[] = [];
    const updateFound = withEventTimeout(
      new Promise<ServiceWorker>((resolve) => {
        registration.addEventListener(
          "updatefound",
          () => {
            lifecycle.push("updatefound");
            const installing = registration.installing;
            assertFixture(installing, "updatefound exposed installing worker");
            lifecycle.push(`installing:${installing.state}`);
            installing.addEventListener("statechange", () => {
              lifecycle.push(`state:${installing.state}`);
              if (installing.state === "activated") {
                resolve(installing);
              }
            });
          },
          { once: true },
        );
      }),
      "service worker updatefound activation",
    );
    const controllerChanged = withEventTimeout(
      new Promise<ServiceWorker>((resolve) => {
        navigator.serviceWorker.addEventListener(
          "controllerchange",
          () => {
            const controller = navigator.serviceWorker.controller;
            if (controller) {
              lifecycle.push("controllerchange");
              resolve(controller);
            }
          },
          { once: true },
        );
      }),
      "service worker update controllerchange",
    );
    const sameRegistration = await navigator.serviceWorker.register(
      workerScriptUrl(token, "v2"),
      { scope: "/", updateViaCache: "none" },
    );
    const [secondWorker, secondController] = await Promise.all([
      updateFound,
      controllerChanged,
    ]);
    assertFixture(sameRegistration === registration, "update reused registration identity");
    assertFixture(secondWorker === registration.active, "updated worker became active");
    assertFixture(secondController !== firstController, "controller identity changed after update");
    assertFixture(urlProjection(secondController.scriptURL).includes("version=v2"), "new controller used v2 script");
    const secondResponse = await fetch("/support/service-worker/version");
    const secondText = await secondResponse.text();
    assertFixture(secondText === `version:v2:${token}`, "updated active worker served v2");
    output(
      root,
      "worker-version-second",
      `${lifecycle.join("|")}\n${urlProjection(secondController.scriptURL)}\n${secondText}`,
    );
    await capturePlatformStep(host, capture, "platform-2", "service-worker-version-two", [
      lifecycle.join(","),
      urlProjection(secondController.scriptURL),
      secondController.state,
      secondText,
    ]);

    return [
      fact("first", firstText),
      fact("second", secondText),
      fact("lifecycle", lifecycle.join("|")),
      fact("same-registration", sameRegistration === registration),
      fact("controller-replaced", secondController !== firstController),
    ];
  } finally {
    await cleanupRegistration(registration);
  }
}

const SCENARIOS: Record<string, ServiceWorkerScenario> = {
  "registration-ready-controller": registrationReadyController,
  "synthetic-fetch-metadata": syntheticFetchMetadata,
  "precache-fetch-roundtrip": precacheFetchRoundtrip,
  "cache-storage-lifecycle": cacheStorageLifecycle,
  "messagechannel-binary-transfer": messageChannelBinaryTransfer,
  "clients-claim-inspection": clientsClaimInspection,
  "streamed-response-clone": streamedResponseClone,
  "fallback-and-redirect": fallbackAndRedirect,
  "unregister-controlled-client": unregisterControlledClient,
  "updatefound-controller-replacement": updatefoundControllerReplacement,
};

export async function runServiceWorkerCacheBoundaryCase(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformBoundaryResult> {
  const scenario = SCENARIOS[spec.slug];
  assertFixture(scenario, `missing service-worker/cache scenario ${spec.slug}`);
  const facts = await scenario(host, meta, spec, capture);
  return { status: "ready", facts };
}

import { assertFixture } from "./harness";
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

type StorageScenario = (
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
) => Promise<PlatformFact[]>;

function scenarioRoot(host: HTMLElement, name: string): HTMLElement {
  const root = document.createElement("section");
  root.dataset.storageScenario = name;
  host.append(root);
  return root;
}

function output(root: HTMLElement, name: string, value: string): HTMLOutputElement {
  const element = document.createElement("output");
  element.dataset.storageOutput = name;
  element.textContent = value;
  root.append(element);
  return element;
}

function storageName(meta: SmokeMeta, spec: CaseSpec, suffix: string): string {
  return `p33-${meta.framework}-${spec.slug}-${spec.seed}-${suffix}`;
}

function requestResult<T>(request: IDBRequest<T>, label: string): Promise<T> {
  return withEventTimeout(
    new Promise<T>((resolve, reject) => {
      request.addEventListener("success", () => resolve(request.result), { once: true });
      request.addEventListener("error", () => reject(request.error ?? new Error(`${label} failed`)), {
        once: true,
      });
    }),
    label,
  );
}

function transactionDone(transaction: IDBTransaction, label: string): Promise<void> {
  return withEventTimeout(
    new Promise<void>((resolve, reject) => {
      transaction.addEventListener("complete", () => resolve(), { once: true });
      transaction.addEventListener(
        "abort",
        () => reject(transaction.error ?? new DOMException(`${label} aborted`, "AbortError")),
        { once: true },
      );
    }),
    label,
  );
}

function openDatabase(
  name: string,
  version: number,
  upgrade: (database: IDBDatabase, transaction: IDBTransaction, event: IDBVersionChangeEvent) => void,
): Promise<IDBDatabase> {
  const request = indexedDB.open(name, version);
  request.addEventListener("upgradeneeded", (rawEvent) => {
    const event = rawEvent as IDBVersionChangeEvent;
    assertFixture(request.transaction, `upgrade transaction exists for ${name}`);
    upgrade(request.result, request.transaction, event);
  });
  return requestResult(request, `open IndexedDB ${name} v${version}`);
}

async function deleteDatabase(name: string): Promise<void> {
  await requestResult(indexedDB.deleteDatabase(name), `delete IndexedDB ${name}`);
}

async function readAll<T>(database: IDBDatabase, storeName: string): Promise<T[]> {
  const transaction = database.transaction(storeName, "readonly");
  const done = transactionDone(transaction, `read ${storeName}`);
  const values = await requestResult(transaction.objectStore(storeName).getAll(), `getAll ${storeName}`);
  await done;
  return values as T[];
}

async function loadSameOriginFrame(root: HTMLElement, label: string): Promise<HTMLIFrameElement> {
  const frame = document.createElement("iframe");
  frame.dataset.storageFrame = label;
  const loaded = withEventTimeout(
    new Promise<void>((resolve) => frame.addEventListener("load", () => resolve(), { once: true })),
    `${label} iframe load`,
  );
  frame.srcdoc = `<!doctype html><html><body data-storage-frame="${label}"></body></html>`;
  root.append(frame);
  await loaded;
  assertFixture(frame.contentWindow, `${label} iframe has a Window`);
  return frame;
}

function waitForStorageEvent(target: Window, key: string, label: string): Promise<StorageEvent> {
  return withEventTimeout(
    new Promise<StorageEvent>((resolve) => {
      const listener = (event: StorageEvent): void => {
        if (event.key !== key) return;
        target.removeEventListener("storage", listener);
        resolve(event);
      };
      target.addEventListener("storage", listener);
    }),
    label,
  );
}

function storageEventLabel(event: StorageEvent): string {
  return [
    event.key,
    event.oldValue,
    event.newValue,
    event.storageArea === localStorage ? "local" : event.storageArea === sessionStorage ? "session" : "foreign",
  ].join(":");
}

async function removeEntryIfPresent(
  directory: FileSystemDirectoryHandle,
  name: string,
  recursive = false,
): Promise<void> {
  try {
    await directory.removeEntry(name, { recursive });
  } catch (error: unknown) {
    if (errorName(error) !== "NotFoundError") throw error;
  }
}

async function writeFile(handle: FileSystemFileHandle, contents: string): Promise<void> {
  const writable = await handle.createWritable();
  await writable.write(contents);
  await writable.close();
}

async function indexeddbUpgradeAbortRollback(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const name = storageName(meta, spec, "upgrade-abort");
  await deleteDatabase(name);
  let database: IDBDatabase | undefined;
  try {
    database = await openDatabase(name, 1, (db) => {
      db.createObjectStore("records", { keyPath: "id" });
    });
    const seedTransaction = database.transaction("records", "readwrite");
    const seeded = transactionDone(seedTransaction, "seed rollback database");
    seedTransaction.objectStore("records").put({ id: 1, value: `stable-${spec.seed}` });
    await seeded;
    const firstValues = await readAll<{ id: number; value: string }>(database, "records");
    const first = `${database.version}|${Array.from(database.objectStoreNames).join(",")}|${firstValues.map((item) => `${item.id}:${item.value}`).join("|")}`;
    output(root, "upgrade-stable", first);
    await capturePlatformStep(host, capture, "platform-1", "indexeddb-version-one", [first]);

    database.close();
    database = undefined;
    const upgrade = indexedDB.open(name, 2);
    upgrade.addEventListener("upgradeneeded", () => {
      upgrade.result.createObjectStore("transient");
      upgrade.transaction?.objectStore("records").put({ id: 2, value: "rolled-back" });
      upgrade.transaction?.abort();
    });
    const upgradeError = await requestResult(upgrade, "aborted IndexedDB upgrade").then(
      () => "resolved",
      (error: unknown) => errorName(error),
    );
    database = await new Promise<IDBDatabase>((resolve, reject) => {
      const reopen = indexedDB.open(name);
      reopen.addEventListener("success", () => resolve(reopen.result), { once: true });
      reopen.addEventListener("error", () => reject(reopen.error), { once: true });
    });
    const finalValues = await readAll<{ id: number; value: string }>(database, "records");
    const second = `${upgradeError}|${database.version}|${Array.from(database.objectStoreNames).join(",")}|${finalValues.map((item) => `${item.id}:${item.value}`).join("|")}`;
    assertFixture(upgradeError === "AbortError", "aborted upgrade rejected with AbortError");
    assertFixture(database.version === 1, "aborted upgrade preserved the old database version");
    assertFixture(!database.objectStoreNames.contains("transient"), "aborted upgrade removed its transient store");
    output(root, "upgrade-rolled-back", second);
    await capturePlatformStep(host, capture, "platform-2", "indexeddb-upgrade-rollback", [second]);

    return [
      fact("initial", first),
      fact("final", second),
      fact("upgrade-error", upgradeError),
      fact("version", database.version),
      fact("records", finalValues.length),
    ];
  } finally {
    database?.close();
    await deleteDatabase(name);
  }
}

async function indexeddbTransactionQueueRollback(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const name = storageName(meta, spec, "transaction-queue");
  await deleteDatabase(name);
  const database = await openDatabase(name, 1, (db) => {
    db.createObjectStore("items", { keyPath: "id" });
  });
  try {
    const order: string[] = [];
    const firstTransaction = database.transaction("items", "readwrite");
    const firstDone = transactionDone(firstTransaction, "first queued transaction").then(() => {
      order.push("first-complete");
    });
    firstTransaction.objectStore("items").put({ id: 1, value: `first-${spec.seed}` });
    firstTransaction.commit();

    const secondTransaction = database.transaction("items", "readwrite");
    const secondDone = transactionDone(secondTransaction, "second queued transaction").then(() => {
      order.push("second-complete");
    });
    const observedFirst = requestResult(
      secondTransaction.objectStore("items").get(1),
      "queued transaction read",
    ).then((value: { id: number; value: string } | undefined) => {
      order.push(`second-read:${value?.value ?? "missing"}`);
      secondTransaction.objectStore("items").put({ id: 2, value: `second-${spec.variant}` });
      return value?.value ?? "missing";
    });
    const seen = await observedFirst;
    await Promise.all([firstDone, secondDone]);
    const firstValues = await readAll<{ id: number; value: string }>(database, "items");
    const first = `${seen}|${order.join("|")}|${firstValues.map((item) => `${item.id}:${item.value}`).join(",")}`;
    assertFixture(order.at(-1) === "second-complete", "overlapping readwrite transactions completed in queue order");
    output(root, "transaction-queue", first);
    await capturePlatformStep(host, capture, "platform-1", "indexeddb-transaction-queue", [first]);

    const abortedTransaction = database.transaction("items", "readwrite");
    const abortedDone = transactionDone(abortedTransaction, "aborted item transaction");
    abortedTransaction.objectStore("items").put({ id: 1, value: "should-rollback" });
    abortedTransaction.objectStore("items").delete(2);
    abortedTransaction.abort();
    const abortError = await abortedDone.then(
      () => "resolved",
      (error: unknown) => errorName(error),
    );
    const afterAbort = await readAll<{ id: number; value: string }>(database, "items");
    const committedTransaction = database.transaction("items", "readwrite");
    const committedDone = transactionDone(committedTransaction, "committed item transaction");
    committedTransaction.objectStore("items").put({ id: 1, value: `committed-${spec.variant}` });
    committedTransaction.commit();
    await committedDone;
    const finalValues = await readAll<{ id: number; value: string }>(database, "items");
    const second = `${abortError}|${afterAbort.map((item) => item.value).join(",")}|${finalValues.map((item) => item.value).join(",")}`;
    assertFixture(abortError === "AbortError", "manual transaction abort rejected with AbortError");
    assertFixture(afterAbort[0]?.value === `first-${spec.seed}` && afterAbort.length === 2, "aborted writes rolled back atomically");
    output(root, "transaction-rollback", second);
    await capturePlatformStep(host, capture, "platform-2", "indexeddb-transaction-rollback", [second]);

    return [
      fact("queue", first),
      fact("rollback", second),
      fact("completion-order", order.join("|")),
      fact("abort-error", abortError),
      fact("final-count", finalValues.length),
    ];
  } finally {
    database.close();
    await deleteDatabase(name);
  }
}

async function indexeddbBlockedVersionDelete(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const name = storageName(meta, spec, "blocked");
  await deleteDatabase(name);
  let current = await openDatabase(name, 1, (db) => db.createObjectStore("v1"));
  let upgraded: IDBDatabase | undefined;
  const events: string[] = [];
  try {
    current.addEventListener("versionchange", (rawEvent) => {
      const event = rawEvent as IDBVersionChangeEvent;
      events.push(`first-versionchange:${event.oldVersion}->${event.newVersion}`);
    });
    const upgradeRequest = indexedDB.open(name, 2);
    const upgradePromise = withEventTimeout(
      new Promise<IDBDatabase>((resolve, reject) => {
        upgradeRequest.addEventListener("blocked", () => {
          events.push("upgrade-blocked");
          current.close();
        });
        upgradeRequest.addEventListener("upgradeneeded", () => {
          events.push("upgrade-start");
          upgradeRequest.result.createObjectStore("v2");
        });
        upgradeRequest.addEventListener("success", () => {
          events.push("upgrade-success");
          resolve(upgradeRequest.result);
        });
        upgradeRequest.addEventListener("error", () => reject(upgradeRequest.error));
      }),
      "blocked IndexedDB upgrade",
    );
    upgraded = await upgradePromise;
    const first = `${events.join("|")}|${upgraded.version}|${Array.from(upgraded.objectStoreNames).join(",")}`;
    assertFixture(events.includes("upgrade-blocked"), "open connection blocked the version upgrade");
    output(root, "blocked-upgrade", first);
    await capturePlatformStep(host, capture, "platform-1", "indexeddb-blocked-upgrade", [first]);

    upgraded.addEventListener("versionchange", (rawEvent) => {
      const event = rawEvent as IDBVersionChangeEvent;
      events.push(`second-versionchange:${event.oldVersion}->${event.newVersion}`);
    });
    const deletion = indexedDB.deleteDatabase(name);
    await withEventTimeout(
      new Promise<void>((resolve, reject) => {
        deletion.addEventListener("blocked", () => {
          events.push("delete-blocked");
          upgraded?.close();
          upgraded = undefined;
        });
        deletion.addEventListener("success", () => {
          events.push("delete-success");
          resolve();
        });
        deletion.addEventListener("error", () => reject(deletion.error));
      }),
      "blocked IndexedDB deletion",
    );
    const databases = await indexedDB.databases();
    const stillPresent = databases.some((info) => info.name === name);
    const second = `${events.join("|")}|present=${stillPresent}`;
    assertFixture(events.includes("delete-blocked"), "open upgraded connection blocked deletion");
    assertFixture(!stillPresent, "successful delete removed database metadata");
    output(root, "blocked-delete", second);
    await capturePlatformStep(host, capture, "platform-2", "indexeddb-blocked-delete", [second]);

    return [
      fact("upgrade", first),
      fact("delete", second),
      fact("events", events.join("|")),
      fact("database-present", stillPresent),
    ];
  } finally {
    current.close();
    upgraded?.close();
    await deleteDatabase(name);
  }
}

async function indexeddbIndexCursorRollback(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const name = storageName(meta, spec, "index-cursor");
  await deleteDatabase(name);
  const database = await openDatabase(name, 1, (db) => {
    const store = db.createObjectStore("records", { keyPath: "id" });
    store.createIndex("tags", "tags", { multiEntry: true });
    store.createIndex("group-rank", ["group", "rank"], { unique: true });
  });

  async function indexSnapshot(): Promise<string> {
    const transaction = database.transaction("records", "readonly");
    const done = transactionDone(transaction, "index snapshot");
    const store = transaction.objectStore("records");
    const red = await requestResult(store.index("tags").getAll("red"), "red multiEntry index");
    const grouped = await requestResult(
      store.index("group-rank").getAll(IDBKeyRange.bound(["alpha", 0], ["alpha", 99])),
      "compound index range",
    );
    await done;
    const redIds = (red as Array<{ id: number }>).map((item) => item.id).join(",");
    const groupedIds = (grouped as Array<{ id: number }>).map((item) => item.id).join(",");
    return `red=${redIds};alpha=${groupedIds}`;
  }

  try {
    const seedTransaction = database.transaction("records", "readwrite");
    const seeded = transactionDone(seedTransaction, "seed indexed records");
    const store = seedTransaction.objectStore("records");
    store.put({ id: 1, group: "alpha", rank: 1, tags: ["red", "blue"], value: `one-${spec.seed}` });
    store.put({ id: 2, group: "alpha", rank: 2, tags: ["red"], value: `two-${spec.variant}` });
    store.put({ id: 3, group: "beta", rank: 1, tags: ["green"], value: "three" });
    await seeded;
    const first = await indexSnapshot();
    output(root, "index-initial", first);
    await capturePlatformStep(host, capture, "platform-1", "indexeddb-index-initial", [first]);

    const abortedTransaction = database.transaction("records", "readwrite");
    const abortedDone = transactionDone(abortedTransaction, "aborted index mutation");
    const abortedStore = abortedTransaction.objectStore("records");
    abortedStore.put({ id: 1, group: "beta", rank: 3, tags: ["green"], value: "aborted" });
    abortedStore.delete(2);
    abortedTransaction.abort();
    const abortError = await abortedDone.then(
      () => "resolved",
      (error: unknown) => errorName(error),
    );
    const afterAbort = await indexSnapshot();

    const commitTransaction = database.transaction("records", "readwrite");
    const committed = transactionDone(commitTransaction, "commit index mutation");
    commitTransaction.objectStore("records").put({
      id: 2,
      group: "beta",
      rank: 2,
      tags: ["green", "blue"],
      value: "committed",
    });
    await committed;
    const finalSnapshot = await indexSnapshot();
    const second = `${abortError}|after=${afterAbort}|final=${finalSnapshot}`;
    assertFixture(afterAbort === first, "aborted object-store writes preserved both indexes");
    assertFixture(finalSnapshot !== first, "committed record update changed index membership");
    output(root, "index-final", second);
    await capturePlatformStep(host, capture, "platform-2", "indexeddb-index-rollback-commit", [second]);

    return [
      fact("initial", first),
      fact("after-abort", afterAbort),
      fact("final", finalSnapshot),
      fact("abort-error", abortError),
    ];
  } finally {
    database.close();
    await deleteDatabase(name);
  }
}

async function cacheDeleteLiveHandle(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const name = storageName(meta, spec, "cache-delete");
  await caches.delete(name);
  try {
    const cache = await caches.open(name);
    const firstUrl = `/support/storage-cache/${spec.seed}?entry=first`;
    const secondUrl = `/support/storage-cache/${spec.seed}?entry=second`;
    await cache.put(firstUrl, new Response(`first-${spec.seed}`, { headers: { "X-Stage": "seed" } }));
    await cache.put(secondUrl, new Response(`second-${spec.variant}`));
    const firstKeys = await cache.keys();
    const firstBody = await (await cache.match(firstUrl))?.text();
    const first = `${firstKeys.map((request) => request.url.split("?").at(-1)).join("|")}|${firstBody}|${await caches.has(name)}`;
    output(root, "cache-before-delete", first);
    await capturePlatformStep(host, capture, "platform-1", "cache-storage-before-delete", [first]);

    const deleted = await caches.delete(name);
    const oldBody = await (await cache.match(secondUrl))?.text();
    await cache.put(`/support/storage-cache/${spec.seed}?entry=detached`, new Response("detached-write"));
    const detachedBody = await (
      await cache.match(`/support/storage-cache/${spec.seed}?entry=detached`)
    )?.text();
    const reopened = await caches.open(name);
    const reopenedKeys = await reopened.keys();
    const second = `${deleted}|has=${await caches.has(name)}|old=${oldBody}|detached=${detachedBody}|reopened=${reopenedKeys.length}`;
    assertFixture(deleted, "CacheStorage.delete removed the named cache");
    assertFixture(oldBody === `second-${spec.variant}`, "deleted Cache handle retained its entries");
    assertFixture(detachedBody === "detached-write", "deleted Cache handle remained writable");
    assertFixture(reopenedKeys.length === 0, "reopening deleted name created a fresh Cache");
    output(root, "cache-after-delete", second);
    await capturePlatformStep(host, capture, "platform-2", "cache-storage-live-handle", [second]);

    return [
      fact("initial", first),
      fact("final", second),
      fact("old-handle-body", oldBody ?? "missing"),
      fact("detached-write", detachedBody ?? "missing"),
      fact("reopened-count", reopenedKeys.length),
    ];
  } finally {
    await caches.delete(name);
  }
}

async function cacheQueryOptionsLifecycle(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const name = storageName(meta, spec, "cache-query");
  await caches.delete(name);
  try {
    const cache = await caches.open(name);
    const base = `/support/cache-query-${spec.seed}`;
    const varyRequest = new Request(`${base}?mode=vary`, { headers: { "X-Mode": "alpha" } });
    await cache.put(`${base}?page=1`, new Response(`page-one-${spec.seed}`));
    await cache.put(`${base}?page=2`, new Response(`page-two-${spec.variant}`));
    await cache.put(varyRequest, new Response("vary-alpha", { headers: { Vary: "X-Mode" } }));

    const ignored = await cache.match(`${base}?page=99`, { ignoreSearch: true });
    const varyMiss = await cache.match(new Request(`${base}?mode=vary`, { headers: { "X-Mode": "beta" } }));
    const varyIgnored = await cache.match(
      new Request(`${base}?mode=vary`, { headers: { "X-Mode": "beta" } }),
      { ignoreVary: true },
    );
    const first = `${await ignored?.text()}|miss=${varyMiss === undefined}|ignored=${await varyIgnored?.text()}|keys=${(await cache.keys()).length}`;
    assertFixture(varyMiss === undefined, "Vary header rejected a mismatched request");
    output(root, "cache-query-first", first);
    await capturePlatformStep(host, capture, "platform-1", "cache-query-options", [first]);

    const deleted = await cache.delete(`${base}?page=1`);
    const matchAll = await cache.matchAll();
    const storageMatch = await caches.match(`${base}?page=2`);
    const second = `${deleted}|keys=${(await cache.keys()).map((request) => request.url.split("?").at(-1)).join("|")}|all=${matchAll.length}|storage=${await storageMatch?.text()}`;
    assertFixture(deleted && matchAll.length === 2, "Cache.delete removed only the exact request");
    output(root, "cache-query-second", second);
    await capturePlatformStep(host, capture, "platform-2", "cache-query-deleted", [second]);

    return [
      fact("queries", first),
      fact("deletion", second),
      fact("deleted", deleted),
      fact("remaining", matchAll.length),
    ];
  } finally {
    await caches.delete(name);
  }
}

async function localStorageCrossRealmEvents(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const key = storageName(meta, spec, "local-event");
  localStorage.removeItem(key);
  const frame = await loadSameOriginFrame(root, "local-events");
  const child = frame.contentWindow as Window;
  try {
    const childEventPromise = waitForStorageEvent(child, key, "child localStorage event");
    localStorage.setItem(key, `parent-${spec.seed}`);
    const childEvent = await childEventPromise;
    const first = `${storageEventLabel(childEvent)}|child=${child.localStorage.getItem(key)}|parent=${localStorage.getItem(key)}`;
    assertFixture(childEvent.storageArea === child.localStorage, "child event exposed the child realm Storage wrapper");
    output(root, "local-parent-write", first);
    await capturePlatformStep(host, capture, "platform-1", "local-storage-parent-write", [first]);

    const parentEventPromise = waitForStorageEvent(window, key, "parent localStorage event");
    child.localStorage.setItem(key, `child-${spec.variant}`);
    const parentEvent = await parentEventPromise;
    const second = `${storageEventLabel(parentEvent)}|child=${child.localStorage.getItem(key)}|parent=${localStorage.getItem(key)}`;
    assertFixture(parentEvent.storageArea === localStorage, "parent event exposed the parent Storage wrapper");
    output(root, "local-child-write", second);
    await capturePlatformStep(host, capture, "platform-2", "local-storage-child-write", [second]);

    return [
      fact("parent-write", first),
      fact("child-write", second),
      fact("final-value", localStorage.getItem(key) ?? "missing"),
      fact("shared-storage", child.localStorage.getItem(key) === localStorage.getItem(key)),
    ];
  } finally {
    localStorage.removeItem(key);
    frame.remove();
  }
}

async function sessionStorageFrameReplacement(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const key = storageName(meta, spec, "session-frame");
  sessionStorage.removeItem(key);
  const firstFrame = await loadSameOriginFrame(root, "session-first");
  const observerFrame = await loadSameOriginFrame(root, "session-observer");
  const firstWindow = firstFrame.contentWindow as Window;
  const observerWindow = observerFrame.contentWindow as Window;
  let replacement: HTMLIFrameElement | undefined;
  try {
    const firstEventPromise = waitForStorageEvent(firstWindow, key, "first frame sessionStorage event");
    const observerEventPromise = waitForStorageEvent(observerWindow, key, "observer frame sessionStorage event");
    sessionStorage.setItem(key, `top-${spec.seed}`);
    const [firstEvent, observerEvent] = await Promise.all([firstEventPromise, observerEventPromise]);
    const first = `${storageEventLabel(firstEvent)}|${storageEventLabel(observerEvent)}|values=${firstWindow.sessionStorage.getItem(key)},${observerWindow.sessionStorage.getItem(key)}`;
    output(root, "session-top-write", first);
    await capturePlatformStep(host, capture, "platform-1", "session-storage-shared", [first]);

    const parentEventPromise = waitForStorageEvent(window, key, "parent sessionStorage removal event");
    firstWindow.sessionStorage.setItem(key, `frame-${spec.variant}`);
    await parentEventPromise;
    firstFrame.remove();
    replacement = await loadSameOriginFrame(root, "session-replacement");
    const replacementWindow = replacement.contentWindow as Window;
    const persisted = replacementWindow.sessionStorage.getItem(key);
    const removalPromise = waitForStorageEvent(window, key, "replacement sessionStorage removal event");
    replacementWindow.sessionStorage.removeItem(key);
    const removal = await removalPromise;
    const second = `persisted=${persisted}|${storageEventLabel(removal)}|top=${sessionStorage.getItem(key)}`;
    assertFixture(persisted === `frame-${spec.variant}`, "replacement frame inherited top-level session storage");
    assertFixture(sessionStorage.getItem(key) === null, "replacement frame removal updated the shared namespace");
    output(root, "session-replacement", second);
    await capturePlatformStep(host, capture, "platform-2", "session-storage-replacement", [second]);

    return [
      fact("shared", first),
      fact("replacement", second),
      fact("persisted", persisted ?? "missing"),
      fact("removed", sessionStorage.getItem(key) === null),
    ];
  } finally {
    sessionStorage.removeItem(key);
    firstFrame.remove();
    observerFrame.remove();
    replacement?.remove();
  }
}

async function opfsWriteTruncateRemove(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const directoryName = storageName(meta, spec, "opfs-directory");
  const opfsRoot = await navigator.storage.getDirectory();
  await removeEntryIfPresent(opfsRoot, directoryName, true);
  try {
    const directory = await opfsRoot.getDirectoryHandle(directoryName, { create: true });
    const file = await directory.getFileHandle("record.txt", { create: true });
    await writeFile(file, `alpha-${spec.seed}|beta-${spec.variant}`);
    const initialFile = await file.getFile();
    const first = `${initialFile.name}|${initialFile.size}|${await initialFile.text()}|${file.kind}`;
    output(root, "opfs-initial", first);
    await capturePlatformStep(host, capture, "platform-1", "opfs-file-written", [first]);

    const writable = await file.createWritable({ keepExistingData: true });
    await writable.seek(6);
    await writable.write(`PATCH-${spec.variant}`);
    await writable.truncate(13);
    await writable.close();
    const patched = await file.getFile();
    const patchedText = await patched.text();
    await directory.removeEntry("record.txt");
    const removedError = await file.getFile().then(
      () => "resolved",
      (error: unknown) => errorName(error),
    );
    const recreated = await directory.getFileHandle("record.txt", { create: true });
    await writeFile(recreated, `recreated-${spec.seed}`);
    const sameEntry = await file.isSameEntry(recreated);
    const recreatedText = await (await recreated.getFile()).text();
    const revivedText = await (await file.getFile()).text();
    const second = `${patched.size}|${patchedText}|${removedError}|same=${sameEntry}|new=${recreatedText}|revived=${revivedText}`;
    assertFixture(removedError === "NotFoundError", "removed OPFS handle no longer resolved a File");
    assertFixture(sameEntry, "recreated path revived the existing path-based OPFS handle");
    assertFixture(revivedText === recreatedText, "revived handle observed the recreated file contents");
    output(root, "opfs-recreated", second);
    await capturePlatformStep(host, capture, "platform-2", "opfs-file-recreated", [second]);

    return [
      fact("initial", first),
      fact("recreated", second),
      fact("removed-error", removedError),
      fact("same-entry", sameEntry),
      fact("directory-kind", directory.kind),
    ];
  } finally {
    await removeEntryIfPresent(opfsRoot, directoryName, true);
  }
}

async function opfsHandleCapabilityClone(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const directoryName = storageName(meta, spec, "opfs-capability");
  const opfsRoot = await navigator.storage.getDirectory();
  await removeEntryIfPresent(opfsRoot, directoryName, true);
  const channel = new MessageChannel();
  try {
    const directory = await opfsRoot.getDirectoryHandle(directoryName, { create: true });
    const original = await directory.getFileHandle("shared.txt", { create: true });
    await writeFile(original, `original-${spec.seed}`);
    const cloned = structuredClone(original);
    const cloneSame = await original.isSameEntry(cloned);
    const first = `${cloneSame}|${cloned.name}|${cloned.kind}|${await (await cloned.getFile()).text()}`;
    assertFixture(cloneSame, "structuredClone preserved OPFS handle entry identity");
    output(root, "opfs-clone", first);
    await capturePlatformStep(host, capture, "platform-1", "opfs-handle-cloned", [first]);

    const portHandle = await withEventTimeout(
      new Promise<FileSystemFileHandle>((resolve) => {
        channel.port1.addEventListener(
          "message",
          (event: MessageEvent<FileSystemFileHandle>) => resolve(event.data),
          { once: true },
        );
        channel.port1.start();
        channel.port2.postMessage(cloned);
      }),
      "MessagePort OPFS handle clone",
    );
    const portSame = await original.isSameEntry(portHandle);
    await writeFile(portHandle, `port-${spec.variant}`);
    const originalAfterPort = await (await original.getFile()).text();
    await directory.removeEntry("shared.txt");
    const staleError = await portHandle.getFile().then(
      () => "resolved",
      (error: unknown) => errorName(error),
    );
    const replacement = await directory.getFileHandle("shared.txt", { create: true });
    const replacementSame = await portHandle.isSameEntry(replacement);
    assertFixture(portSame, "MessagePort clone preserved OPFS handle identity");
    await writeFile(replacement, `replacement-${spec.seed}`);
    const revivedText = await (await portHandle.getFile()).text();
    const second = `${portSame}|${originalAfterPort}|${staleError}|replacement=${replacementSame}|revived=${revivedText}`;
    assertFixture(staleError === "NotFoundError", "removed capability failed before path recreation");
    assertFixture(replacementSame && revivedText === `replacement-${spec.seed}`, "path recreation revived all cloned capabilities");
    output(root, "opfs-port-clone", second);
    await capturePlatformStep(host, capture, "platform-2", "opfs-handle-stale", [second]);

    return [
      fact("structured-clone", first),
      fact("message-clone", second),
      fact("port-same-entry", portSame),
      fact("stale-error", staleError),
      fact("replacement-same-entry", replacementSame),
    ];
  } finally {
    channel.port1.close();
    channel.port2.close();
    await removeEntryIfPresent(opfsRoot, directoryName, true);
  }
}

const SCENARIOS: Record<string, StorageScenario> = {
  "indexeddb-upgrade-abort-rollback": indexeddbUpgradeAbortRollback,
  "indexeddb-transaction-queue-rollback": indexeddbTransactionQueueRollback,
  "indexeddb-blocked-version-delete": indexeddbBlockedVersionDelete,
  "indexeddb-index-cursor-rollback": indexeddbIndexCursorRollback,
  "cache-delete-live-handle": cacheDeleteLiveHandle,
  "cache-query-options-lifecycle": cacheQueryOptionsLifecycle,
  "local-storage-cross-realm-events": localStorageCrossRealmEvents,
  "session-storage-frame-replacement": sessionStorageFrameReplacement,
  "opfs-write-truncate-remove": opfsWriteTruncateRemove,
  "opfs-handle-capability-clone": opfsHandleCapabilityClone,
};

export async function runStorageLifecycleBoundaryCase(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformBoundaryResult> {
  const scenario = SCENARIOS[spec.slug];
  assertFixture(scenario, `missing storage lifecycle scenario ${spec.slug}`);
  const facts = await scenario(host, meta, spec, capture);
  return { status: "ready", facts };
}

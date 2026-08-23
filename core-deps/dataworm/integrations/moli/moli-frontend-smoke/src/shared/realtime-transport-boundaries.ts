import { assertFixture, expectNetworkFailure } from "./harness";
import {
  capturePlatformStep,
  fact,
  type CapturePlatformFrame,
  type PlatformBoundaryResult,
  type PlatformFact,
  withEventTimeout,
} from "./platform-boundary";
import type { CaseSpec, SmokeMeta } from "./types";

type RealtimeScenario = (
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
) => Promise<PlatformFact[]>;

interface RealtimeConfig {
  websocketUrl: string;
}

interface WebSocketStatus {
  active: number;
  closed: boolean;
  opened: number;
  token: string;
}

interface EventSourceStatus extends WebSocketStatus {
  lastEventIds: string[];
}

function scenarioRoot(host: HTMLElement, name: string): HTMLElement {
  const root = document.createElement("section");
  root.dataset.realtimeScenario = name;
  host.append(root);
  return root;
}

function output(root: HTMLElement, name: string, value: string): HTMLOutputElement {
  const element = document.createElement("output");
  element.dataset.realtimeOutput = name;
  element.textContent = value;
  root.append(element);
  return element;
}

function scenarioToken(meta: SmokeMeta, spec: CaseSpec): string {
  return `p34-${meta.framework}-${spec.slug}-${spec.seed}`;
}

async function jsonResponse<T>(url: string): Promise<T> {
  const response = await fetch(url, { cache: "no-store" });
  assertFixture(response.ok, `${new URL(response.url).pathname} returned ${response.status}`);
  return response.json() as Promise<T>;
}

async function websocketUrl(scenario: string, token: string): Promise<string> {
  const config = await jsonResponse<RealtimeConfig>("/support/realtime/config");
  const url = new URL(config.websocketUrl);
  url.searchParams.set("scenario", scenario);
  url.searchParams.set("token", token);
  return url.href;
}

function realtimeUrl(pathname: string, values: Record<string, string>): string {
  return `${pathname}?${new URLSearchParams(values)}`;
}

function eventSourceUrl(scenario: string, token: string): string {
  const url = realtimeUrl("/support/realtime/events", { scenario, token });
  expectNetworkFailure({
    label: `eventsource-close-${scenario}`,
    url: new URL(url, location.href).href,
    type: "EventSource",
    canceled: true,
  });
  return url;
}

function socketOpen(socket: WebSocket, label: string): Promise<Event> {
  return withEventTimeout(
    new Promise<Event>((resolve, reject) => {
      const cleanup = (): void => {
        socket.removeEventListener("open", onOpen);
        socket.removeEventListener("error", onError);
      };
      const onOpen = (event: Event): void => {
        cleanup();
        resolve(event);
      };
      const onError = (): void => {
        cleanup();
        reject(new Error(`${label} emitted error before open`));
      };
      socket.addEventListener("open", onOpen);
      socket.addEventListener("error", onError);
    }),
    label,
  );
}

function socketMessage(socket: WebSocket, label: string): Promise<MessageEvent<unknown>> {
  return withEventTimeout(
    new Promise<MessageEvent<unknown>>((resolve, reject) => {
      const cleanup = (): void => {
        socket.removeEventListener("message", onMessage as EventListener);
        socket.removeEventListener("error", onError);
      };
      const onMessage = (event: MessageEvent<unknown>): void => {
        cleanup();
        resolve(event);
      };
      const onError = (): void => {
        cleanup();
        reject(new Error(`${label} emitted error before message`));
      };
      socket.addEventListener("message", onMessage as EventListener);
      socket.addEventListener("error", onError);
    }),
    label,
  );
}

function socketClose(socket: WebSocket, label: string): Promise<CloseEvent> {
  return withEventTimeout(
    new Promise<CloseEvent>((resolve) => {
      socket.addEventListener("close", (event) => resolve(event), { once: true });
    }),
    label,
  );
}

async function closeSocket(
  socket: WebSocket,
  token: string,
  code = 1000,
  reason = "fixture-complete",
): Promise<{ close: CloseEvent; status: WebSocketStatus }> {
  const closed = socketClose(socket, `${token} websocket close`);
  socket.close(code, reason);
  const close = await closed;
  const status = await jsonResponse<WebSocketStatus>(
    realtimeUrl("/support/realtime/websocket-status", { token }),
  );
  assertFixture(status.closed && status.active === 0, `${token} websocket closed on server`);
  return { close, status };
}

function eventSourceOpen(source: EventSource, label: string): Promise<Event> {
  return withEventTimeout(
    new Promise<Event>((resolve) => {
      source.addEventListener("open", (event) => resolve(event), { once: true });
    }),
    label,
  );
}

function eventSourceMessage(
  source: EventSource,
  eventName: string,
  label: string,
): Promise<MessageEvent<string>> {
  return withEventTimeout(
    new Promise<MessageEvent<string>>((resolve) => {
      source.addEventListener(
        eventName,
        (event) => resolve(event as MessageEvent<string>),
        { once: true },
      );
    }),
    label,
  );
}

async function closeEventSource(
  source: EventSource,
  token: string,
): Promise<{ released: boolean; status: EventSourceStatus }> {
  source.close();
  const release = await jsonResponse<{ released: boolean }>(
    realtimeUrl("/support/realtime/release-event-source", { token }),
  );
  const status = await jsonResponse<EventSourceStatus>(
    realtimeUrl("/support/realtime/event-source-status", { token }),
  );
  assertFixture(status.closed && status.active === 0, `${token} event source closed on server`);
  return { released: release.released, status };
}

function bytes(value: ArrayBuffer): string {
  return [...new Uint8Array(value)].join(",");
}

async function websocketTextOrder(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = scenarioToken(meta, spec);
  const socket = new WebSocket(await websocketUrl("text-order", token));
  const opened = socketOpen(socket, "text websocket open");
  const greetingPromise = socketMessage(socket, "text websocket greeting");
  await opened;
  const greeting = String((await greetingPromise).data);
  assertFixture(greeting === `server-open:${token}`, "websocket received its server greeting");
  output(root, "text-open", `${greeting}|state=${socket.readyState}`);
  await capturePlatformStep(host, capture, "platform-1", "websocket-text-open", [
    greeting,
    socket.readyState,
  ]);

  const alphaPromise = socketMessage(socket, "text websocket alpha echo");
  socket.send(`alpha:${meta.framework}`);
  const alpha = String((await alphaPromise).data);
  const betaPromise = socketMessage(socket, "text websocket beta echo");
  socket.send(`beta:${spec.variant}`);
  const beta = String((await betaPromise).data);
  const { close, status } = await closeSocket(socket, token);
  assertFixture(
    alpha === `echo:1:alpha:${meta.framework}` && beta === `echo:2:beta:${spec.variant}`,
    "text websocket preserved message and echo order",
  );
  output(root, "text-echo", `${alpha}|${beta}|${close.code}|${close.wasClean}`);
  await capturePlatformStep(host, capture, "platform-2", "websocket-text-echo-close", [
    alpha,
    beta,
    close.code,
    close.wasClean,
    status.opened,
  ]);

  return [
    fact("greeting", greeting),
    fact("echoes", `${alpha}|${beta}`),
    fact("close", `${close.code}|${close.wasClean}`),
    fact("server-opened", status.opened),
  ];
}

async function websocketBinaryArrayBuffer(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = scenarioToken(meta, spec);
  const socket = new WebSocket(await websocketUrl("binary-arraybuffer", token));
  socket.binaryType = "arraybuffer";
  await socketOpen(socket, "binary websocket open");
  const firstPromise = socketMessage(socket, "binary websocket first frame");
  socket.send(new Uint8Array([1, 3, 5, 7]).buffer);
  const firstData = (await firstPromise).data;
  assertFixture(firstData instanceof ArrayBuffer, "binary websocket projected ArrayBuffer data");
  const first = bytes(firstData);
  assertFixture(first === "7,5,3,1", "binary websocket reversed the first byte frame");
  output(root, "binary-first", `${socket.binaryType}|${first}`);
  await capturePlatformStep(host, capture, "platform-1", "websocket-arraybuffer-first", [
    socket.binaryType,
    first,
  ]);

  const secondPromise = socketMessage(socket, "binary websocket second frame");
  socket.send(new Uint8Array([0, 128, 255]));
  const secondData = (await secondPromise).data;
  assertFixture(secondData instanceof ArrayBuffer, "second binary frame remained ArrayBuffer");
  const second = bytes(secondData);
  const { close, status } = await closeSocket(socket, token);
  assertFixture(second === "255,128,0", "binary websocket preserved unsigned bytes");
  output(root, "binary-second", `${second}|${close.code}|opened=${status.opened}`);
  await capturePlatformStep(host, capture, "platform-2", "websocket-arraybuffer-second", [
    second,
    close.code,
    status.opened,
  ]);

  return [fact("first", first), fact("second", second), fact("binary-type", socket.binaryType)];
}

async function websocketFragmentedBinary(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = scenarioToken(meta, spec);
  const socket = new WebSocket(await websocketUrl("binary-fragmented", token));
  const firstPromise = socketMessage(socket, "fragmented websocket blob");
  await socketOpen(socket, "fragmented websocket open");
  const firstData = (await firstPromise).data;
  assertFixture(firstData instanceof Blob, "default binaryType projected a Blob");
  const first = bytes(await firstData.arrayBuffer());
  assertFixture(first === "0,1,254,255", "fragmented websocket message reassembled into one Blob");
  output(root, "fragmented-blob", `${firstData.type}|${firstData.size}|${first}`);
  await capturePlatformStep(host, capture, "platform-1", "websocket-fragments-blob", [
    firstData.type,
    firstData.size,
    first,
  ]);

  socket.binaryType = "arraybuffer";
  const secondPromise = socketMessage(socket, "fragmented websocket arraybuffer");
  socket.send("blob-received");
  const secondData = (await secondPromise).data;
  assertFixture(secondData instanceof ArrayBuffer, "updated binaryType affected the next message");
  const second = bytes(secondData);
  socket.send("arraybuffer-received");
  const { close } = await closeSocket(socket, token);
  assertFixture(second === "16,32,48", "second fragmented message was reassembled");
  output(root, "fragmented-buffer", `${socket.binaryType}|${second}|${close.wasClean}`);
  await capturePlatformStep(host, capture, "platform-2", "websocket-fragments-arraybuffer", [
    socket.binaryType,
    second,
    close.wasClean,
  ]);

  return [
    fact("blob", `${firstData.size}|${first}`),
    fact("arraybuffer", second),
    fact("clean-close", close.wasClean),
  ];
}

async function websocketSubprotocolMetadata(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = scenarioToken(meta, spec);
  const socket = new WebSocket(await websocketUrl("subprotocol", token), [
    "smoke.v2",
    "smoke.v1",
  ]);
  const messagePromise = socketMessage(socket, "subprotocol metadata");
  await socketOpen(socket, "subprotocol websocket open");
  const payload = JSON.parse(String((await messagePromise).data)) as {
    originHost: string;
    protocol: string;
    token: string;
  };
  assertFixture(socket.protocol === "smoke.v2", "server selected its preferred subprotocol");
  assertFixture(payload.protocol === socket.protocol, "protocol matched server metadata");
  assertFixture(payload.originHost === location.hostname, "websocket sent the document Origin");
  output(root, "subprotocol", `${socket.protocol}|${payload.originHost}|${payload.token}`);
  await capturePlatformStep(host, capture, "platform-1", "websocket-subprotocol", [
    socket.protocol,
    payload.originHost,
    payload.token,
  ]);

  const { close, status } = await closeSocket(socket, token);
  output(root, "subprotocol-close", `${close.code}|${close.reason}|${status.opened}`);
  await capturePlatformStep(host, capture, "platform-2", "websocket-subprotocol-close", [
    close.code,
    close.reason,
    close.wasClean,
    status.opened,
  ]);

  return [
    fact("protocol", socket.protocol),
    fact("origin-host", payload.originHost),
    fact("server-opened", status.opened),
  ];
}

async function websocketCloseHandshake(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = scenarioToken(meta, spec);
  const clientToken = `${token}-client`;
  const clientSocket = new WebSocket(await websocketUrl("client-close", clientToken));
  const clientReady = socketMessage(clientSocket, "client-close ready");
  await socketOpen(clientSocket, "client-close websocket open");
  assertFixture(
    (await clientReady).data === `client-close-ready:${clientToken}`,
    "client-close fixture became ready",
  );
  const clientResult = await closeSocket(clientSocket, clientToken, 3001, "client-done");
  assertFixture(
    clientResult.close.code === 3001 &&
      clientResult.close.reason === "client-done" &&
      clientResult.close.wasClean,
    "client initiated a clean application close",
  );
  output(
    root,
    "client-close",
    `${clientResult.close.code}|${clientResult.close.reason}|${clientResult.close.wasClean}`,
  );
  await capturePlatformStep(host, capture, "platform-1", "websocket-client-close", [
    clientResult.close.code,
    clientResult.close.reason,
    clientResult.close.wasClean,
    clientResult.status.opened,
  ]);

  const serverToken = `${token}-server`;
  const serverSocket = new WebSocket(await websocketUrl("server-close", serverToken));
  const serverReady = socketMessage(serverSocket, "server-close ready");
  const serverClosed = socketClose(serverSocket, "server initiated websocket close");
  await socketOpen(serverSocket, "server-close websocket open");
  assertFixture(
    (await serverReady).data === `server-close-ready:${serverToken}`,
    "server-close fixture became ready",
  );
  const close = await serverClosed;
  const status = await jsonResponse<WebSocketStatus>(
    realtimeUrl("/support/realtime/websocket-status", { token: serverToken }),
  );
  assertFixture(
    close.code === 3002 && close.reason === "server-done" && close.wasClean,
    "server initiated a clean application close",
  );
  assertFixture(status.closed && status.active === 0, "server close released its fixture socket");
  output(root, "server-close", `${close.code}|${close.reason}|${close.wasClean}`);
  await capturePlatformStep(host, capture, "platform-2", "websocket-server-close", [
    close.code,
    close.reason,
    close.wasClean,
    status.opened,
  ]);

  return [
    fact("client-close", `${clientResult.close.code}|${clientResult.close.reason}`),
    fact("server-close", `${close.code}|${close.reason}`),
    fact("clean", clientResult.close.wasClean && close.wasClean),
  ];
}

async function websocketIframeRealmTeardown(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = scenarioToken(meta, spec);
  const url = await websocketUrl("realm-teardown", token);
  const frame = document.createElement("iframe");
  frame.title = "websocket realm owner";
  frame.dataset.realtimeFrame = "websocket-owner";
  root.append(frame);
  const frameWindow = frame.contentWindow;
  assertFixture(frameWindow, "websocket iframe exposes a Window");
  const loaded = withEventTimeout(
    new Promise<void>((resolve) => {
      frame.addEventListener("load", () => resolve(), { once: true });
    }),
    "websocket iframe load",
  );
  const ready = withEventTimeout(
    new Promise<{ message: string; state: number }>((resolve) => {
      const listener = (event: MessageEvent<unknown>): void => {
        const data = event.data as { source?: string; message?: string; state?: number } | null;
        if (event.source !== frameWindow || data?.source !== "realtime-frame-ready") {
          return;
        }
        window.removeEventListener("message", listener);
        resolve({ message: data.message ?? "missing", state: data.state ?? -1 });
      };
      window.addEventListener("message", listener);
    }),
    "iframe websocket ready message",
  );
  frame.srcdoc = `<!doctype html><html><head><meta charset="utf-8"><title>websocket owner</title></head><body><main><output id="state">connecting</output></main><script>addEventListener("message",(command)=>{if(command.data?.source!=="realtime-frame-start")return;const socket=new WebSocket(command.data.url);socket.addEventListener("message",(event)=>{document.querySelector("#state").textContent=event.data;parent.postMessage({source:"realtime-frame-ready",message:event.data,state:socket.readyState},"*");},{once:true});},{once:true});<\/script></body></html>`;
  await loaded;
  frameWindow.postMessage({ source: "realtime-frame-start", url }, "*");
  const frameReady = await ready;
  assertFixture(
    frameReady.message === `realm-ready:${token}` && frameReady.state === WebSocket.OPEN,
    "iframe owned an open websocket",
  );
  output(root, "realm-open", `${frameReady.message}|${frameReady.state}`);
  await capturePlatformStep(host, capture, "platform-1", "websocket-iframe-open", [
    frameReady.message,
    frameReady.state,
    frame.contentDocument?.title,
  ]);

  frame.remove();
  await new Promise<void>((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  });
  const status = await jsonResponse<WebSocketStatus>(
    realtimeUrl("/support/realtime/websocket-status", { token }),
  );
  assertFixture(
    status.closed && status.active === 0 && status.opened === 1,
    "destroying the iframe closed its realm-owned websocket",
  );
  output(root, "realm-closed", `${status.active}|${status.closed}|${status.opened}`);
  await capturePlatformStep(host, capture, "platform-2", "websocket-iframe-destroyed", [
    frame.isConnected,
    status.active,
    status.closed,
    status.opened,
  ]);

  return [
    fact("open-state", frameReady.state),
    fact("frame-connected", frame.isConnected),
    fact("server-closed", status.closed),
    fact("server-opened", status.opened),
  ];
}

async function eventSourceMultilineCustom(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = scenarioToken(meta, spec);
  const source = new EventSource(eventSourceUrl("multiline-custom", token));
  const opened = eventSourceOpen(source, "multiline EventSource open");
  const updatePromise = eventSourceMessage(source, "update", "custom EventSource update");
  const defaultPromise = eventSourceMessage(source, "message", "default EventSource message");
  await opened;
  const update = await updatePromise;
  assertFixture(
    update.data === "first line\nsecond café" && update.lastEventId === `custom-${token}`,
    "EventSource joined multiline custom event data",
  );
  output(root, "sse-custom", `${update.type}|${update.lastEventId}|${update.data}`);
  await capturePlatformStep(host, capture, "platform-1", "eventsource-custom-multiline", [
    update.type,
    update.lastEventId,
    update.data,
    source.readyState,
  ]);

  const message = await defaultPromise;
  const closed = await closeEventSource(source, token);
  assertFixture(
    message.data === "default 東京" && message.lastEventId === `default-${token}`,
    "EventSource dispatched its default message after the custom event",
  );
  assertFixture(closed.released, "multiline EventSource released its held response");
  output(
    root,
    "sse-default",
    `${message.type}|${message.lastEventId}|${message.data}|${source.readyState}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "eventsource-default-close", [
    message.type,
    message.lastEventId,
    message.data,
    source.readyState,
    closed.status.opened,
  ]);

  return [
    fact("custom", `${update.lastEventId}|${update.data}`),
    fact("default", `${message.lastEventId}|${message.data}`),
    fact("closed-state", source.readyState),
    fact("last-request-ids", closed.status.lastEventIds.join("|")),
  ];
}

async function eventSourceReconnectLastId(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = scenarioToken(meta, spec);
  const source = new EventSource(eventSourceUrl("reconnect-last-id", token));
  const firstPromise = eventSourceMessage(source, "message", "first reconnect event");
  await eventSourceOpen(source, "reconnecting EventSource first open");
  const first = await firstPromise;
  const secondPromise = eventSourceMessage(source, "message", "second reconnect event");
  assertFixture(
    first.data === `first:${token}` && first.lastEventId === `first-${token}`,
    "first EventSource connection installed its event ID",
  );
  output(root, "sse-reconnect-first", `${first.lastEventId}|${first.data}`);
  await capturePlatformStep(host, capture, "platform-1", "eventsource-first-connection", [
    first.lastEventId,
    first.data,
  ]);

  const second = await secondPromise;
  const closed = await closeEventSource(source, token);
  assertFixture(
    second.data === `second:last=first-${token}` && second.lastEventId === `second-${token}`,
    "reconnected EventSource sent Last-Event-ID and received the next event",
  );
  assertFixture(
    closed.status.opened === 2 &&
      closed.status.lastEventIds.join("|") === `|first-${token}`,
    "fixture observed exactly one reconnect with Last-Event-ID",
  );
  output(
    root,
    "sse-reconnect-second",
    `${second.lastEventId}|${second.data}|requests=${closed.status.opened}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "eventsource-last-event-id", [
    second.lastEventId,
    second.data,
    closed.status.opened,
    closed.status.lastEventIds.join(","),
  ]);

  return [
    fact("first", `${first.lastEventId}|${first.data}`),
    fact("second", `${second.lastEventId}|${second.data}`),
    fact("request-ids", closed.status.lastEventIds.join("|")),
    fact("opened", closed.status.opened),
  ];
}

async function eventSourceCloseReadyState(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = scenarioToken(meta, spec);
  const source = new EventSource(eventSourceUrl("close-state", token));
  const messagePromise = eventSourceMessage(source, "message", "close-state EventSource event");
  await eventSourceOpen(source, "close-state EventSource open");
  const openState = Number(source.readyState);
  assertFixture(openState === EventSource.OPEN, "EventSource exposed OPEN after open");
  output(root, "sse-open-state", `${openState}|${source.withCredentials}`);
  await capturePlatformStep(host, capture, "platform-1", "eventsource-open-state", [
    openState,
    source.withCredentials,
  ]);

  const message = await messagePromise;
  const closed = await closeEventSource(source, token);
  source.close();
  const closedState = Number(source.readyState);
  assertFixture(closedState === EventSource.CLOSED, "EventSource close was idempotent");
  assertFixture(
    message.data === `close-state:${token}` && message.lastEventId === `close-${token}`,
    "close-state EventSource delivered its terminal fixture event",
  );
  output(
    root,
    "sse-closed-state",
    `${closedState}|${message.lastEventId}|${message.data}|${closed.status.closed}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "eventsource-closed-state", [
    closedState,
    message.lastEventId,
    message.data,
    closed.status.closed,
  ]);

  return [
    fact("message", `${message.lastEventId}|${message.data}`),
    fact("closed-state", closedState),
    fact("server-closed", closed.status.closed),
  ];
}

async function coordinatedWebSocketEventSource(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const token = scenarioToken(meta, spec);
  const socket = new WebSocket(await websocketUrl("coordinated", token));
  const socketOpened = socketOpen(socket, "coordinated websocket open");
  const socketReadyPromise = socketMessage(socket, "coordinated websocket ready");
  const source = new EventSource(eventSourceUrl("coordinated", token));
  const sourceOpened = eventSourceOpen(source, "coordinated EventSource open");
  const sourceReadyPromise = eventSourceMessage(
    source,
    "message",
    "coordinated EventSource ready",
  );
  await Promise.all([socketOpened, sourceOpened]);
  const [socketReadyEvent, sourceReady] = await Promise.all([
    socketReadyPromise,
    sourceReadyPromise,
  ]);
  const socketReady = String(socketReadyEvent.data);
  assertFixture(socketReady === `websocket-ready:${token}`, "coordinated websocket opened");
  assertFixture(
    sourceReady.data === `eventsource-ready:${token}`,
    "coordinated EventSource opened",
  );
  output(
    root,
    "coordinated-open",
    `${socketReady}|${sourceReady.lastEventId}|${sourceReady.data}`,
  );
  await capturePlatformStep(host, capture, "platform-1", "realtime-transports-open", [
    socketReady,
    sourceReady.lastEventId,
    sourceReady.data,
    socket.readyState,
    source.readyState,
  ]);

  const echoPromise = socketMessage(socket, "coordinated websocket echo");
  socket.send(`bridge:${meta.framework}:${spec.variant}`);
  const echo = String((await echoPromise).data);
  const [socketResult, sourceResult] = await Promise.all([
    closeSocket(socket, token),
    closeEventSource(source, token),
  ]);
  assertFixture(
    echo === `coordinated:bridge:${meta.framework}:${spec.variant}`,
    "coordinated websocket echoed after the EventSource event",
  );
  assertFixture(sourceResult.released, "coordinated EventSource released its response");
  output(
    root,
    "coordinated-close",
    `${echo}|ws=${socketResult.status.closed}|sse=${sourceResult.status.closed}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "realtime-transports-closed", [
    echo,
    socketResult.close.code,
    socketResult.status.closed,
    source.readyState,
    sourceResult.status.closed,
  ]);

  return [
    fact("socket-ready", socketReady),
    fact("event-source-ready", sourceReady.data),
    fact("echo", echo),
    fact("both-closed", socketResult.status.closed && sourceResult.status.closed),
  ];
}

const SCENARIOS: Record<string, RealtimeScenario> = {
  "websocket-text-order": websocketTextOrder,
  "websocket-binary-arraybuffer": websocketBinaryArrayBuffer,
  "websocket-fragmented-binary": websocketFragmentedBinary,
  "websocket-subprotocol-origin": websocketSubprotocolMetadata,
  "websocket-close-handshake": websocketCloseHandshake,
  "websocket-iframe-teardown": websocketIframeRealmTeardown,
  "eventsource-multiline-custom": eventSourceMultilineCustom,
  "eventsource-reconnect-last-id": eventSourceReconnectLastId,
  "eventsource-close-ready-state": eventSourceCloseReadyState,
  "websocket-eventsource-coordination": coordinatedWebSocketEventSource,
};

export async function runRealtimeTransportBoundaryCase(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformBoundaryResult> {
  const scenario = SCENARIOS[spec.slug];
  assertFixture(scenario, `missing realtime transport scenario ${spec.slug}`);
  const facts = await scenario(host, meta, spec, capture);
  return { status: "ready", facts };
}

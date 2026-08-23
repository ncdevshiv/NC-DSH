import { createSignal, onCleanup, onMount, type JSX } from "solid-js";
import Header from "./components/Header";
import SummaryPanel from "./components/SummaryPanel";
import GraphCanvas from "./components/GraphCanvas";
import FeedPanel from "./components/FeedPanel";
import DetailDrawer from "./components/DetailDrawer";
import Console from "./components/Console";
import { TOKEN, api } from "./api";
import { GraphModel } from "./graph";
import type {
  BusEvent,
  ChangeReport,
  ContextResult,
  GraphSnapshot,
  Ping,
  SearchHit,
  Summary,
} from "./types";

const EDGE_TYPES = new Set(["contains", "references", "duplicate_of", "similar_to"]);
const FEED_CAP = 300;
const ACTIVITY_CAP = 40;

const base = (p: string) => (p || "").slice((p || "").lastIndexOf("/") + 1);

export default function App(): JSX.Element {
  // ---- global state ---------------------------------------------------------
  const model = new GraphModel();
  const [conn, setConn] = createSignal<"connecting" | "live" | "reconnecting">("connecting");
  const [ping, setPing] = createSignal<Ping | null>(null);
  const [summary, setSummary] = createSignal<Summary | null>(null);
  const [crawl, setCrawl] = createSignal<{ converged: boolean | null; cycles: number }>({
    converged: null,
    cycles: 0,
  });
  const [feed, setFeed] = createSignal<ChangeReport[]>([]);
  const [activity, setActivity] = createSignal<{ seq: number; kind: string; text: string }[]>([]);
  const [drawerPath, setDrawerPath] = createSignal("");
  const [drawerLoading, setDrawerLoading] = createSignal(false);
  const [drawerData, setDrawerData] = createSignal<ContextResult | null>(null);
  const [selectedId, setSelectedId] = createSignal<string | null>(null);
  const [focusNonce, setFocusNonce] = createSignal(0);
  const [tracePath, setTracePath] = createSignal<string | null>(null);
  let es: EventSource | null = null;
  let lastSeq = 0; // highest seq applied; ring-buffer replays on reconnect are dropped

  async function refreshSummary(): Promise<void> {
    try {
      setSummary(await api<Summary>("summary"));
    } catch {
      /* transient (crawl lock / daemon restart); next done retries */
    }
  }

  function tick(kind: string, text: string, seq: number): void {
    setActivity((prev) => [{ seq, kind, text }, ...prev].slice(0, ACTIVITY_CAP));
  }

  function onFocusPath(path: string): void {
    setSelectedId(path);
    setFocusNonce((n) => n + 1);
    setDrawerPath(path);
    setDrawerData(null);
    setDrawerLoading(true);
    // Pull the node's 2-hop neighbourhood into the rendered set so the canvas
    // focuses meaningfully even when the cap evicted this region earlier.
    api<{ neighbors?: SearchHit[] | { id: string }[] }>("neighbors", { path, depth: 2 })
      .then((r) => {
        const ids = [path, ...(r.neighbors ?? []).map((n) => String(n.id))];
        model.focusIds(ids);
      })
      .catch(() => {
        model.focusIds([path]);
      });
    api<ContextResult>("context", { path })
      .then((ctx) => setDrawerData(ctx))
      .catch((e) =>
        setDrawerData({ error: e instanceof Error ? e.message : String(e) }),
      )
      .finally(() => setDrawerLoading(false));
  }

  /** Cold-start bootstrap: replay the persisted graph so a restarted daemon
   *  (whose SSE ring only has post-start events) still paints the canvas. */
  async function loadSnapshot(): Promise<void> {
    try {
      const snap = await api<GraphSnapshot>("graph", { max_edges: 30000 });
      model.ingestBatch(snap.nodes ?? []);
      for (const e of snap.edges ?? []) {
        const t = String(e.edge_type ?? "");
        if (EDGE_TYPES.has(t)) model.ingestEdge(String(e.src), String(e.dst), t as never);
      }
      if (snap.edges_truncated) {
        tick("graph", `snapshot · ${snap.edges_truncated ?? 0} low-value edges skipped`, Number.MAX_SAFE_INTEGER);
      }
    } catch {
      /* old daemon without /api/graph — live stream remains the only source */
    }
  }

  // ---- SSE lifecycle ----------------------------------------------------------
  onMount(() => {
    void api<Ping>("ping").then(setPing).catch(() => setPing({ ok: false }));
    void refreshSummary();
    void loadSnapshot();

    es = new EventSource(`/events?token=${encodeURIComponent(TOKEN)}`);
    es.onopen = () => {
      setConn("live");
      // Daemon restart mid-session: the replay carries nothing from before the
      // restart, so re-bootstrap if the canvas never got any nodes.
      if (model.totalSeen === 0) void loadSnapshot();
    };
    es.onerror = () => setConn("reconnecting"); // EventSource retries itself
    es.onmessage = (m: MessageEvent<string>) => {
      let ev: BusEvent;
      try {
        ev = JSON.parse(m.data) as BusEvent;
      } catch {
        return;
      }
      const seq = Number(ev.seq);
      if (Number.isFinite(seq)) {
        if (seq <= lastSeq) return; // replayed event — already applied
        lastSeq = seq;
      }
      switch (ev.kind) {
        case "change": {
          const report = ev.report as ChangeReport | undefined;
          if (report) setFeed((prev) => [{ ...report, seq: ev.seq }, ...prev].slice(0, FEED_CAP));
          break;
        }
        case "done": {
          if (ev.reason === "fs_event") {
            // micro-recrawl resync: refresh summary, keep crawl convergence/cycles
            tick("resync", "⚡ resync", ev.seq);
            void refreshSummary();
            break;
          }
          setCrawl({
            converged: Boolean(ev.converged),
            cycles: Number(ev.cycles ?? crawl().cycles),
          });
          tick("done", `✓ done · cycles ${ev.cycles ?? "?"}${ev.converged ? "" : " · diverged"}`, ev.seq);
          void refreshSummary();
          break;
        }
        case "start":
          tick("start", `▶ crawl ${base(String(ev.root ?? ""))}`, ev.seq);
          break;
        case "pass":
          tick("pass", `pass ${ev.name} · c${ev.cycle} ${ev.status}`, ev.seq);
          break;
        case "cycle":
          setCrawl((c) => ({ ...c, cycles: Number(ev.n ?? c.cycles) }));
          tick("cycle", `cycle ${ev.n} settled`, ev.seq);
          break;
        case "progress":
          tick("progress", `discovered ${ev.discovered}`, ev.seq);
          break;
        case "node":
          model.ingestNode(String(ev.id), String(ev.node_kind ?? "file"));
          break;
        case "nodes_batch":
          model.ingestBatch((ev.nodes as { id: string; node_kind?: string }[]) ?? []);
          break;
        case "edge": {
          const t = String(ev.edge_type ?? ev.type ?? "");
          if (EDGE_TYPES.has(t)) {
            model.ingestEdge(String(ev.src), String(ev.dst), t as never);
          }
          break;
        }
        case "reset_dim":
          model.resetDim(String(ev.edge_type ?? "") as never);
          tick("reset_dim", `reset ${ev.edge_type}`, ev.seq);
          break;
        case "cross_links":
          tick("cross_links", `${ev.count} cross-dir links`, ev.seq);
          break;
        default:
          if (ev.kind.startsWith("fs_")) {
            tick(ev.kind, `⚡${ev.kind.slice(3)} ${base(String(ev.path ?? ""))}`, ev.seq);
          } else if (ev.kind === "cross_dir_impact") {
            tick("cross_dir_impact", "cross-dir impact sweep", ev.seq);
          } else if (ev.kind === "dropped") {
            tick("dropped", `⚠ ${ev.count} events dropped (slow client)`, ev.seq);
          }
      }
    };
  });
  onCleanup(() => es?.close());

  return (
    <div class="app">
      <Header
        conn={conn}
        ping={ping}
        summary={summary}
        cycles={() => crawl().cycles}
        converged={() => crawl().converged}
      />
      <div class="main">
        <SummaryPanel summary={summary} />
        <div class="center-wrap glass">
          <GraphCanvas
            model={model}
            selectedId={selectedId()}
            focusNonce={focusNonce()}
            onFocusPath={onFocusPath}
          />
          <DetailDrawer
            path={drawerPath}
            loading={drawerLoading}
            data={drawerData}
            onClose={() => {
              setDrawerPath("");
              setSelectedId(null);
            }}
            onTrace={(p) => setTracePath(p)}
          />
        </div>
        <FeedPanel feed={feed} activity={activity} />
      </div>
      <Console tracePath={tracePath} onConsumedTrace={() => setTracePath(null)} />
    </div>
  );
}

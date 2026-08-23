import { createEffect, createSignal, onCleanup, onMount, Show } from "solid-js";
import type { JSX } from "solid-js";
import { api } from "../api";
import type { GraphModel } from "../graph";
import type { SearchHit } from "../types";

const EDGE_COLOR: Record<string, string> = {
  contains: "rgba(100,116,139,0.28)",
  references: "rgba(56,189,248,0.55)",
  duplicate_of: "rgba(251,146,60,0.6)",
  similar_to: "rgba(192,132,252,0.55)",
};

export default function GraphCanvas(props: {
  model: GraphModel;
  selectedId: string | null;
  focusNonce: number;
  onFocusPath: (path: string) => void;
}): JSX.Element {
  let canvas!: HTMLCanvasElement;
  let wrap!: HTMLDivElement;
  const view = { ox: 0, oy: 0, scale: 1 };
  const [counts, setCounts] = createSignal({ shown: 0, total: 0 });
  const [query, setQuery] = createSignal("");
  const [hits, setHits] = createSignal<SearchHit[]>([]);
  const [searching, setSearching] = createSignal(false);
  let drag: { sx: number; sy: number; ox: number; oy: number; moved: boolean } | null = null;

  // ---- viewport helpers ----------------------------------------------------
  const toWorld = (sx: number, sy: number): [number, number] => [
    (sx - view.ox) / view.scale,
    (sy - view.oy) / view.scale,
  ];

  const resize = () => {
    const dpr = window.devicePixelRatio || 1;
    const w = wrap.clientWidth;
    const h = wrap.clientHeight;
    canvas.width = Math.max(1, Math.floor(w * dpr));
    canvas.height = Math.max(1, Math.floor(h * dpr));
    canvas.style.width = `${w}px`;
    canvas.style.height = `${h}px`;
  };

  const centerOn = (wx: number, wy: number, scale?: number) => {
    if (scale) view.scale = scale;
    view.ox = wrap.clientWidth / 2 - wx * view.scale;
    view.oy = wrap.clientHeight / 2 - wy * view.scale;
  };

  /** Center + scale the viewport around the current rendered cloud. */
  const fitView = (): void => {
    const M = props.model;
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const id of M.order) {
      const n = M.nodes.get(id);
      if (!n || !M.rendered.has(id)) continue;
      if (n.x < minX) minX = n.x;
      if (n.y < minY) minY = n.y;
      if (n.x > maxX) maxX = n.x;
      if (n.y > maxY) maxY = n.y;
    }
    if (!Number.isFinite(minX) || !Number.isFinite(minY)) return;
    const w = wrap.clientWidth || 800;
    const h = wrap.clientHeight || 600;
    const bw = Math.max(maxX - minX, 60);
    const bh = Math.max(maxY - minY, 60);
    const s = Math.min(2, Math.max(0.15, Math.min((w * 0.85) / bw, (h * 0.85) / bh)));
    view.scale = s;
    view.ox = w / 2 - ((minX + maxX) / 2) * s;
    view.oy = h / 2 - ((minY + maxY) / 2) * s;
  };

  // Focus request from parent (search pick / feed click).
  createEffect(() => {
    if (!props.focusNonce) return;
    const sel = props.selectedId;
    const n = sel ? props.model.nodes.get(sel) : null;
    if (n) {
      centerOn(n.x, n.y, Math.max(view.scale, 0.9));
      setHits([]);
      setQuery("");
    }
  });

  // ---- render loop ---------------------------------------------------------
  onMount(() => {
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(wrap);
    let raf = 0;
    const draw = () => {
      props.model.tick();
      const dpr = window.devicePixelRatio || 1;
      const g = canvas.getContext("2d");
      if (g) {
        g.setTransform(dpr, 0, 0, dpr, 0, 0);
        g.clearRect(0, 0, canvas.width, canvas.height);
        g.translate(view.ox, view.oy);
        g.scale(view.scale, view.scale);
        const M = props.model;
        // edges
        for (const e of M.edges.values()) {
          const a = M.nodes.get(e.src);
          const b = M.nodes.get(e.dst);
          if (!a || !b || !M.rendered.has(e.src) || !M.rendered.has(e.dst)) continue;
          g.strokeStyle = EDGE_COLOR[e.type] ?? EDGE_COLOR.contains;
          g.lineWidth = e.type === "contains" ? 1 : 1.6;
          g.beginPath();
          g.moveTo(a.x, a.y);
          g.lineTo(b.x, b.y);
          g.stroke();
        }
        // nodes
        for (const id of M.order) {
          const n = M.nodes.get(id);
          if (!n || !M.rendered.has(id)) continue;
          const isDir = n.kind === "dir";
          g.beginPath();
          g.arc(n.x, n.y, isDir ? 7 : 4, 0, Math.PI * 2);
          g.fillStyle = isDir ? "#f59e0b" : "#38bdf8";
          g.fill();
          if (id === props.selectedId) {
            g.beginPath();
            g.arc(n.x, n.y, isDir ? 12 : 9, 0, Math.PI * 2);
            g.strokeStyle = "#e2e8f0";
            g.lineWidth = 2;
            g.stroke();
          }
        }
        // dir labels only when the graph is sparse or we're zoomed in
        if (M.renderedCount <= 140 || view.scale > 1.6) {
          g.fillStyle = "rgba(226,232,240,0.75)";
          g.font = "11px ui-monospace, monospace";
          g.textAlign = "center";
          for (const id of M.order) {
            const n = M.nodes.get(id);
            if (n && M.rendered.has(id) && n.kind === "dir") {
              g.fillText(id.slice(id.lastIndexOf("/") + 1), n.x, n.y - 10);
            }
          }
        }
      }
      const c = props.model.counts;
      if (c.shown !== counts().shown || c.total !== counts().total) {
        // First population: fit the viewport to the freshly spawned cloud so
        // the graph opens centered instead of anchored at the top-left corner
        // (world origin) where most nodes start off-screen.
        if (counts().shown === 0 && c.shown > 0) fitView();
        setCounts({ ...c });
      }
      raf = requestAnimationFrame(draw);
    };
    raf = requestAnimationFrame(draw);
    onCleanup(() => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      clearTimeout(searchTimer); // pending debounce must not fire post-dispose
    });

    // wheel zoom around cursor
    const onWheel = (ev: WheelEvent) => {
      ev.preventDefault();
      const rect = canvas.getBoundingClientRect();
      const mx = ev.clientX - rect.left;
      const my = ev.clientY - rect.top;
      const [wx, wy] = toWorld(mx, my);
      view.scale = Math.min(6, Math.max(0.08, view.scale * Math.pow(1.0015, -ev.deltaY)));
      view.ox = mx - wx * view.scale;
      view.oy = my - wy * view.scale;
    };
    canvas.addEventListener("wheel", onWheel, { passive: false });
    onCleanup(() => canvas.removeEventListener("wheel", onWheel));
  });

  // ---- pointer interaction ---------------------------------------------------
  const pick = (sx: number, sy: number): string | null => {
    const [wx, wy] = toWorld(sx, sy);
    let best: string | null = null;
    let bestD = Infinity;
    for (const id of props.model.order) {
      const n = props.model.nodes.get(id);
      if (!n || !props.model.rendered.has(id)) continue;
      const r = n.kind === "dir" ? 10 : 7;
      const d = (n.x - wx) ** 2 + (n.y - wy) ** 2;
      if (d < r * r && d < bestD) {
        bestD = d;
        best = id;
      }
    }
    return best;
  };

  const onPointerDown = (ev: PointerEvent) => {
    const rect = canvas.getBoundingClientRect();
    drag = {
      sx: ev.clientX - rect.left,
      sy: ev.clientY - rect.top,
      ox: view.ox,
      oy: view.oy,
      moved: false,
    };
    canvas.setPointerCapture(ev.pointerId);
  };
  const onPointerMove = (ev: PointerEvent) => {
    if (!drag) return;
    const rect = canvas.getBoundingClientRect();
    const mx = ev.clientX - rect.left;
    const my = ev.clientY - rect.top;
    if (Math.abs(mx - drag.sx) + Math.abs(my - drag.sy) > 4) drag.moved = true;
    if (drag.moved) {
      view.ox = drag.ox + (mx - drag.sx);
      view.oy = drag.oy + (my - drag.sy);
    }
  };
  const onPointerUp = (ev: PointerEvent) => {
    if (drag && !drag.moved) {
      const rect = canvas.getBoundingClientRect();
      const id = pick(ev.clientX - rect.left, ev.clientY - rect.top);
      if (id) props.onFocusPath(id);
    }
    drag = null;
  };

  // ---- search ----------------------------------------------------------------
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  const runSearch = (text: string) => {
    clearTimeout(searchTimer);
    if (!text.trim()) {
      setHits([]);
      return;
    }
    setSearching(true);
    searchTimer = setTimeout(async () => {
      try {
        const r = await api<{ results: SearchHit[] }>("search", { text: text.trim(), limit: 8 });
        setHits(r.results ?? []);
      } catch {
        setHits([]);
      } finally {
        setSearching(false);
      }
    }, 220);
  };

  return (
    <div class="graph-wrap" ref={wrap}>
      <canvas
        ref={canvas}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
      />
      <div class="graph-toolbar">
        <input
          class="glass-input"
          placeholder="search paths…"
          value={query()}
          onInput={(e) => {
            setQuery(e.currentTarget.value);
            runSearch(e.currentTarget.value);
          }}
        />
        <Show when={hits().length}>
          <div class="search-drops glass">
            {hits().map((h) => (
              <button class="search-hit" onClick={() => props.onFocusPath(h.id)} title={h.path || h.id}>
                <span class={`badge kind-${h.kind}`}>{h.kind}</span>
                <span class="mono">{h.id}</span>
              </button>
            ))}
          </div>
        </Show>
      </div>
      <div class="graph-note mono">
        showing {counts().shown} of {counts().total} nodes · scroll=zoom · drag=pan · click=node
      </div>
      <Show when={counts().total === 0}>
        <div class="graph-empty">
          waiting for the worm… run <code>dataworm crawl &lt;dir&gt;</code> and the stream will
          populate this canvas live.
        </div>
      </Show>
    </div>
  );
}

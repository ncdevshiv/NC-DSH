import { For, Show, type JSX } from "solid-js";
import type { Summary } from "../types";

const DIMS: { key: string; label: string; color: string }[] = [
  { key: "edges_contains", label: "contains", color: "#64748b" },
  { key: "edges_references", label: "references", color: "#38bdf8" },
  { key: "edges_duplicate_of", label: "duplicate_of", color: "#fb923c" },
  { key: "edges_similar_to", label: "similar_to", color: "#c084fc" },
];

export default function SummaryPanel(props: { summary: () => Summary | null }): JSX.Element {
  const s = () => props.summary();
  const kinds = () => s()?.node_kinds ?? {};
  const num = (v: unknown): string => (typeof v === "number" ? String(v) : "—");
  const basename = (p: string): string => p.split(/[\\/]/).filter(Boolean).pop() ?? p;

  return (
    <aside class="panel left-panel glass">
      <h2>Summary</h2>
      <div class="stats-grid">
        <div class="stat-card">
          <div class="stat-value">{num(s()?.nodes)}</div>
          <div class="stat-label">nodes</div>
        </div>
        <div class="stat-card">
          <div class="stat-value">{num(s()?.edges)}</div>
          <div class="stat-label">edges</div>
        </div>
        <div class="stat-card">
          <div class="stat-value" style="color:#38bdf8">{num(kinds()["file"])}</div>
          <div class="stat-label">files</div>
        </div>
        <div class="stat-card">
          <div class="stat-value" style="color:#f59e0b">{num(kinds()["dir"])}</div>
          <div class="stat-label">dirs</div>
        </div>
      </div>
      <h3>dimensions</h3>
      <ul class="dim-list">
        <For each={DIMS}>
          {(d) => (
            <li>
              <span class="swatch" style={`background:${d.color}`} />
              <span class="dim-name">{d.label}</span>
              <span class="mono dim-count">{num((s() as Record<string, unknown> | undefined)?.[d.key])}</span>
            </li>
          )}
        </For>
      </ul>
      <Show when={(s()?.fragments?.length ?? 0) > 1}>
        <h3>fragments</h3>
        <ul class="dim-list">
          <For each={s()?.fragments ?? []}>
            {(f) => (
              <li>
                <span class="dim-name mono">{basename(f.root)}</span>
                <span class="mono dim-count">
                  {f.nodes}n · {f.edges}e
                </span>
              </li>
            )}
          </For>
        </ul>
        <Show when={typeof s()?.shadows === "number"}>
          <p class="hint">
            {s()!.fragments!.length} fragments · shadows: {s()!.shadows}
          </p>
        </Show>
      </Show>
      <p class="hint">
        refreshed on load and after every <code>done</code> event.
      </p>
    </aside>
  );
}

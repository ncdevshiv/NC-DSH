import { For, Show as Show0, createMemo, createSignal, type JSX } from "solid-js";
import type { ChangeReport } from "../types";

const KIND_CLASS: Record<string, string> = {
  created: "k-created",
  modified: "k-modified",
  deleted: "k-deleted",
  moved: "k-moved",
  burst: "k-burst",
};

function hhmmss(ts?: number): string {
  if (!ts) return "";
  const d = new Date(ts * 1000);
  return d.toLocaleTimeString([], { hour12: false });
}

const base = (p: string) => p.slice(p.lastIndexOf("/") + 1) || p;

export default function FeedPanel(props: {
  feed: () => ChangeReport[];
  activity: () => { seq: number; kind: string; text: string }[];
}): JSX.Element {
  const [filter, setFilter] = createSignal("");
  const filtered = createMemo(() => {
    const f = filter().trim().toLowerCase();
    const rows = props.feed();
    if (!f) return rows;
    return rows.filter(
      (r) =>
        r.path?.toLowerCase().includes(f) ||
        r.kind.toLowerCase().includes(f) ||
        r.root?.toLowerCase().includes(f),
    );
  });

  return (
    <section class="panel right-panel glass">
      <h2>LIVE · Reflex Arc</h2>
      {/* progress ticks during crawls */}
      <div class="activity-strip mono" title="pass / cycle / done / fs ticks">
        <For each={props.activity().slice(0, 8)}>{(a) => <span class={`tick t-${a.kind}`}>{a.text}</span>}</For>
        <Show0 when={props.activity().length === 0}>
          <span class="tick muted">no crawl activity yet…</span>
        </Show0>
      </div>

      <input
        class="glass-input"
        placeholder="filter change reports (path / kind / root)"
        value={filter()}
        onInput={(e) => setFilter(e.currentTarget.value)}
      />

      <div class="feed-list">
        <For each={filtered().slice(0, 200)}>
          {(r, i) => (
            <article class={`feed-row ${KIND_CLASS[r.kind] ?? "k-burst"}`}>
              <div class="row-top">
                <span class={`badge ${KIND_CLASS[r.kind] ?? "k-burst"}`}>{r.kind}</span>
                <span class="mono path" title={`${r.root || ""}/${r.path}`}>
                  {base(r.path || "")}
                </span>
                <time>{hhmmss(r.ts)}</time>
              </div>
              <Show0 when={r.kind === "burst" && r.paths?.length}>
                <div class="row-sub mono">{r.paths!.length} paths aggregated (report cap)</div>
              </Show0>
              <div class="row-sub mono">
                <Show0 when={r.refs_gained?.length}>
                  <span class="delta plus" title={r.refs_gained!.join("\n")}>+{r.refs_gained!.length} refs</span>
                </Show0>
                <Show0 when={r.refs_lost?.length}>
                  <span class="delta minus" title={r.refs_lost!.join("\n")}>−{r.refs_lost!.length} refs</span>
                </Show0>
                <Show0 when={r.dependents_after !== undefined}>
                  <span class="delta deps" title={(r.dependents_after ?? []).join("\n")}>
                    ⇩{(r.dependents_after ?? []).length} dependents
                  </span>
                </Show0>
                <Show0 when={r.dangling_now?.length}>
                  <span class="delta dangling">⚠ {r.dangling_now!.length} dangling</span>
                </Show0>
                <span class="seq">#{r.seq ?? i()}</span>
              </div>
            </article>
          )}
        </For>
        <Show0 when={filtered().length === 0}>
          <div class="empty-note">no change reports{filter() ? " match the filter" : " yet — edit watched files"}.</div>
        </Show0>
      </div>
    </section>
  );
}

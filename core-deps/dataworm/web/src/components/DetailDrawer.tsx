import { For, Show, type JSX } from "solid-js";
import type { ContextResult } from "../types";

const TYPE_COLOR: Record<string, string> = {
  contains: "#64748b",
  references: "#38bdf8",
  duplicate_of: "#fb923c",
  similar_to: "#c084fc",
};

export default function DetailDrawer(props: {
  data: () => ContextResult | null;
  loading: () => boolean;
  path: () => string;
  onClose: () => void;
  onTrace: (path: string) => void;
}): JSX.Element {
  const ctx = () => props.data();
  return (
    <Show when={props.path()}>
      <div class="drawer glass" role="dialog">
        <div class="drawer-head">
          <span class="mono drawer-path" title={props.path()}>
            {props.path().slice(props.path().lastIndexOf("/") + 1)}
          </span>
          <button class="icon-btn" onClick={props.onClose} title="close">✕</button>
        </div>
        <Show when={props.loading()}>
          <div class="empty-note">loading details…</div>
        </Show>
        <Show when={!props.loading() && ctx()?.error}>
          <div class="empty-note err">{ctx()?.error}</div>
        </Show>
        <Show when={!props.loading() && ctx() && !ctx()!.error}>
          <div class="kv mono"><span class="k">id</span><span class="v">{ctx()!.node?.id}</span></div>
          <div class="kv mono"><span class="k">kind</span><span class="v">{ctx()!.node?.kind}</span></div>
          <Show when={ctx()!.node?.size !== undefined}>
            <div class="kv mono"><span class="k">size</span><span class="v">{ctx()!.node!.size} B</span></div>
          </Show>

          <h3>link counts</h3>
          <div class="chip-row">
            <For each={Object.entries(ctx()!.link_counts ?? {})}>
              {([t, n]) => (
                <span class="chip" style={`border-color:${TYPE_COLOR[t] ?? "rgba(255,255,255,.2)"}`}>
                  {t}: <b>{n}</b>
                </span>
              )}
            </For>
            <Show when={!Object.keys(ctx()!.link_counts ?? {}).length}>
              <span class="empty-note">no links</span>
            </Show>
          </div>

          <Show when={(ctx()!.dangling_references ?? []).length}>
            <h3>dangling references</h3>
            <ul class="mini-list mono">
              <For each={ctx()!.dangling_references!}>{(d) => <li>⚠ {d}</li>}</For>
            </ul>
          </Show>

          <h3>links</h3>
          <ul class="mini-list mono links-list">
            <For each={(ctx()!.links ?? []).slice(0, 60)}>
              {(l) => (
                <li>
                  <span style={`color:${TYPE_COLOR[l.type] ?? "inherit"}`}>
                    {l.direction === "out" ? "→" : "←"} {l.type}
                  </span>{" "}
                  {l.id}
                  <Show when={l.cross_dir}> <span class="badge k-moved">cross-dir</span></Show>
                </li>
              )}
            </For>
            <Show when={(ctx()!.links ?? []).length > 60}>
              <li class="muted">… {(ctx()!.links ?? []).length - 60} more</li>
            </Show>
          </ul>

          <Show when={ctx()!.impact && !(ctx()!.impact as { error?: string }).error}>
            <h3>impact snapshot</h3>
            <button class="btn" onClick={() => props.onTrace(props.path())}>
              open full impact →
            </button>
            <div class="mono impact-inline">
              {(ctx()!.impact as { total_affected?: number }).total_affected ?? 0} affected ·{" "}
              direct {(ctx()!.impact as { direct?: string[] }).direct?.length ?? 0}
            </div>
          </Show>
        </Show>
      </div>
    </Show>
  );
}

import { For, Show, createEffect, createSignal, type JSX } from "solid-js";
import { api, rpc } from "../api";
import type { ImpactResult, PlanEditResult } from "../types";

type Tab = "impact" | "plan";

export default function Console(props: {
  tracePath: () => string | null;
  onConsumedTrace: () => void;
}): JSX.Element {
  const [tab, setTab] = createSignal<Tab>("impact");

  // ---- impact state ----
  const [ipath, setIpath] = createSignal("");
  const [impact, setImpact] = createSignal<ImpactResult | null>(null);
  const [ierr, setIerr] = createSignal("");
  const [ibsy, setIbsy] = createSignal(false);

  const runImpact = async (p?: string) => {
    const path = (p ?? ipath()).trim();
    if (!path) return;
    setIpath(path);
    setIbsy(true);
    setIerr("");
    try {
      setImpact(await api<ImpactResult>("impact", { path }));
    } catch (e) {
      setImpact(null);
      setIerr(String(e instanceof Error ? e.message : e));
    } finally {
      setIbsy(false);
    }
  };

  // a node click can push a path into the console ("trace impact")
  createEffect(() => {
    const p = props.tracePath();
    if (p) {
      setTab("impact");
      void runImpact(p);
      props.onConsumedTrace();
    }
  });

  // ---- plan_edit state ----
  const [ppath, setPpath] = createSignal("");
  const [content, setContent] = createSignal("");
  const [plan, setPlan] = createSignal<PlanEditResult | null>(null);
  const [perr, setPerr] = createSignal("");
  const [pbsy, setPbsy] = createSignal(false);

  const runPlan = async () => {
    if (!ppath().trim() || !content()) return;
    setPbsy(true);
    setPerr("");
    try {
      setPlan(await rpc<PlanEditResult>("plan_edit", { path: ppath().trim(), content: content() }));
    } catch (e) {
      setPlan(null);
      setPerr(String(e instanceof Error ? e.message : e));
    } finally {
      setPbsy(false);
    }
  };

  return (
    <section class="console glass">
      <div class="tabs">
        <button classList={{ tab: true, active: tab() === "impact" }} onClick={() => setTab("impact")}>
          Impact console
        </button>
        <button classList={{ tab: true, active: tab() === "plan" }} onClick={() => setTab("plan")}>
          PlanEdit simulator
        </button>
        <div class="spacer" />
        <Show when={ibsy() || pbsy()}>
          <span class="spin">⟳</span>
        </Show>
      </div>

      <Show when={tab() === "impact"}>
        <form
          class="console-bar"
          onSubmit={(e) => {
            e.preventDefault();
            void runImpact();
          }}
        >
          <input
            class="glass-input mono grow"
            placeholder="trace impact of a file…"
            value={ipath()}
            onInput={(e) => setIpath(e.currentTarget.value)}
          />
          <button class="btn primary" type="submit">trace blast radius</button>
        </form>
        <Show when={ierr()}><div class="empty-note err">{ierr()}</div></Show>
        <Show when={impact()}>
          <div class="impact-grid">
            <div class="imp-col">
              <h3><span class="badge k-created">direct</span> {impact()!.direct?.length ?? 0}</h3>
              <ul class="mono mini-list">
                <For each={impact()!.direct ?? []}>{(d) => <li>{d}</li>}</For>
                <Show when={!impact()!.direct?.length}><li class="muted">none</li></Show>
              </ul>
            </div>
            <div class="imp-col">
              <h3><span class="badge k-modified">transitive</span> {impact()!.transitive?.length ?? 0}</h3>
              <ul class="mono mini-list">
                <For each={impact()!.transitive ?? []}>{(t) => <li>{t}</li>}</For>
                <Show when={!impact()!.transitive?.length}><li class="muted">none</li></Show>
              </ul>
            </div>
          </div>
          <div class="mono impact-total">
            target <b>{impact()!.target}</b> · total affected <b>{impact()!.total_affected ?? 0}</b>
            <Show when={impact()!.truncated}> · truncated at cap</Show>
          </div>
        </Show>
      </Show>

      <Show when={tab() === "plan"}>
        <div class="plan-grid">
          <div class="plan-left">
            <input
              class="glass-input mono"
              placeholder="path to simulate (e.g. src/app.py or brand/new.py)"
              value={ppath()}
              onInput={(e) => setPpath(e.currentTarget.value)}
            />
            <textarea
              class="glass-input mono plan-content"
              placeholder="proposed file content…"
              spellcheck={false}
              value={content()}
              onInput={(e) => setContent(e.currentTarget.value)}
            />
            <button class="btn primary" disabled={pbsy()} onClick={() => void runPlan()}>
              simulate plan edit…
            </button>
          </div>
          <div class="plan-right">
            <Show when={perr()}><div class="empty-note err">{perr()}</div></Show>
            <Show when={plan()}>
              <div class="chip-row">
                <span class="delta plus">+{plan()!.refs_gained?.length ?? 0} refs gained</span>
                <span class="delta minus">−{plan()!.refs_lost?.length ?? 0} refs lost</span>
                <span class="delta deps">⇩{plan()!.dependents_count ?? 0} dependents</span>
                <Show when={plan()!.exact_duplicate_of}>
                  <span class="badge k-deleted">dup of {plan()!.exact_duplicate_of}</span>
                </Show>
                <Show when={plan()!.unchanged}>
                  <span class="badge k-burst">unchanged</span>
                </Show>
              </div>
              <pre class="pretty-json mono">{JSON.stringify(plan(), null, 2)}</pre>
            </Show>
            <Show when={!plan() && !perr()}>
              <div class="empty-note">
                pure what-if: diffs would-be references + duplication radar against the live graph.
                never writes disk.
              </div>
            </Show>
          </div>
        </div>
      </Show>
    </section>
  );
}

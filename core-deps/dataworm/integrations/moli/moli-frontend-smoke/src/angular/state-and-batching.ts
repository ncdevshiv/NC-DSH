import { Component, computed, signal, type Type } from "@angular/core";

import { money, stableItems } from "../shared/data";
import type { CaseSpec, SmokeMeta } from "../shared/types";
import { bootstrapAngular } from "./support";

interface StateComponent {
  count: ReturnType<typeof signal<number>>;
  enabled: ReturnType<typeof signal<boolean>>;
  reduced: ReturnType<typeof signal<number>>;
}

function componentType(meta: SmokeMeta, spec: CaseSpec): Type<StateComponent> {
  class AngularStateComponent implements StateComponent {
    readonly meta = meta;
    readonly spec = spec;
    readonly count = signal(0);
    readonly enabled = signal(false);
    readonly reduced = signal(spec.seed);
    readonly items = stableItems(spec.seed, Math.max(4, spec.size));
    readonly total = computed(() => this.items.reduce((sum, item) => sum + item.amount, 0) + this.count());
    readonly money = money;
    readonly metrics = Array.from({ length: 16 }, (_, index) => index);
    summaries() { return [["count", String(this.count())], ["enabled", String(this.enabled())], ["reduced", String(this.reduced())], ["total", money(this.total())]]; }
  }
  Component({
    selector: "smoke-root",
    standalone: true,
    template: `
      <main id="smoke-root" data-framework="angular" [attr.data-family]="meta.family" [attr.data-count]="count()">
        <h1>{{ meta.title }}</h1><section data-case-body>
          @switch (spec.variant) {
            @case (0) { <output aria-label="count">{{ count() }}</output> }
            @case (1) { <p [attr.data-batch-result]="count()">Three queued increments: {{ count() }}</p> }
            @case (2) { <p>Derived total <strong>{{ money(total()) }}</strong></p> }
            @case (3) { <div role="switch" [attr.aria-checked]="enabled()" [attr.data-state]="enabled() ? 'on' : 'off'">Feature {{ enabled() ? "enabled" : "disabled" }}</div> }
            @case (4) { <p>Reducer value <b>{{ reduced() }}</b></p> }
            @case (5) { <dl><dt>Gross</dt><dd>{{ money(total()) }}</dd><dt>Net</dt><dd>{{ money(total() - reduced()) }}</dd></dl> }
            @case (6) { <ol>@for (summary of summaries(); track summary[0]) { <li [attr.data-key]="summary[0]">{{ summary[0] }}: {{ summary[1] }}</li> }</ol> }
            @case (7) { <section><h2>Computed summary</h2>@for (item of items.slice(0, 8); track item.id) { <p>{{ item.title }}: {{ money(item.amount + count()) }}</p> }</section> }
            @case (8) { <div [attr.data-machine-state]="enabled() ? 'settled' : 'booting'"><h2>{{ enabled() ? "Settled" : "Booting" }}</h2><p>Transition #{{ reduced() }}</p></div> }
            @default { <div class="metrics">@for (index of metrics; track index) { <article><h2>Metric {{ index + 1 }}</h2><strong>{{ total() + index * reduced() }}</strong><small>{{ enabled() ? "live" : "idle" }}</small></article> }</div> }
          }
        </section>
      </main>
    `,
  })(AngularStateComponent);
  return AngularStateComponent;
}

export async function mount(meta: SmokeMeta, spec: CaseSpec): Promise<void> {
  await bootstrapAngular(componentType(meta, spec), meta, "angular-batched-signal-update", (instance) => {
    instance.count.update((value) => value + 1);
    instance.count.update((value) => value + 2);
    instance.count.update((value) => value + spec.variant + 3);
    instance.enabled.set(true);
    instance.reduced.update((value) => value + 5 + spec.variant);
  });
}

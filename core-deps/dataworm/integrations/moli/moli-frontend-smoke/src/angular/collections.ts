import { Component, signal, type Type } from "@angular/core";

import { money, stableItems, type StableItem } from "../shared/data";
import type { CaseSpec, SmokeMeta } from "../shared/types";
import { bootstrapAngular } from "./support";

interface CollectionsComponent {
  items: ReturnType<typeof signal<StableItem[]>>;
  source: StableItem[];
}

function componentType(meta: SmokeMeta, spec: CaseSpec): Type<CollectionsComponent> {
  class AngularCollectionsComponent implements CollectionsComponent {
    readonly meta = meta;
    readonly spec = spec;
    readonly source = stableItems(spec.seed, spec.variant === 9 ? 48 : Math.max(5, spec.size));
    readonly items = signal([...this.source]);
    readonly statuses = ["new", "active", "paused", "done"] as const;
    readonly String = String;
    readonly money = money;
    byStatus(status: string) { return this.items().filter((item) => item.status === status); }
  }
  Component({
    selector: "smoke-root",
    standalone: true,
    template: `
      <main id="smoke-root" data-framework="angular" [attr.data-family]="meta.family">
        <h1>{{ meta.title }}</h1><section data-case-body>
          @if (spec.variant <= 3) {
            <ul [attr.data-count]="items().length">@for (item of items(); track item.id) { <li [attr.data-id]="item.id" [attr.data-status]="item.status"><strong>{{ item.title }}</strong><span>{{ item.owner }}</span></li> }</ul>
          } @else if (spec.variant === 4) {
            @for (status of statuses; track status) { <section><h2>{{ status }}</h2><ul>@for (item of byStatus(status); track item.id) { <li><strong>{{ item.title }}</strong><span>{{ item.owner }}</span></li> }</ul></section> }
          } @else if (spec.variant === 5) {
            <ul>@for (item of items().slice(0, 6); track item.id; let index = $index) { <li><span>{{ item.title }}</span>@if (index < 3) { <ul><li>{{ item.owner }}</li><li>{{ item.tags.join(" / ") }}</li></ul> }</li> }</ul>
          } @else if (spec.variant === 6 || spec.variant === 9) {
            <table><thead><tr><th>Item</th><th>Owner</th><th>Status</th><th>Amount</th></tr></thead><tbody>@for (item of items(); track item.id) { <tr><th scope="row">{{ item.title }}</th><td>{{ item.owner }}</td><td>{{ item.status }}</td><td>{{ money(item.amount) }}</td></tr> }</tbody></table>
          } @else if (spec.variant === 7) {
            <div role="list" class="card-grid">@for (item of items(); track item.id) { <article role="listitem"><h2>{{ item.title }}</h2><p>{{ item.owner }}</p><div>@for (tag of item.tags; track tag) { <span>{{ tag }}</span> }</div></article> }</div>
          } @else {
            <ol class="activity-feed">@for (item of items(); track item.id; let index = $index) { <li><time [attr.datetime]="'2026-07-' + String(index + 1).padStart(2, '0')">Day {{ index + 1 }}</time><p><b>{{ item.owner }}</b> moved {{ item.title }} to {{ item.status }}</p></li> }</ol>
          }
        </section>
      </main>
    `,
  })(AngularCollectionsComponent);
  return AngularCollectionsComponent;
}

export async function mount(meta: SmokeMeta, spec: CaseSpec): Promise<void> {
  await bootstrapAngular(componentType(meta, spec), meta, "angular-keyed-collection-update", (instance) => {
    if (spec.variant === 1) instance.items.set([...instance.source].reverse());
    else if (spec.variant === 2) instance.items.set([{ ...instance.source[0], id: `prepended-${spec.seed}`, title: "Prepended checkpoint" }, ...instance.source]);
    else if (spec.variant === 3) instance.items.set(instance.source.filter((_, index) => index % 2 === 0));
    else instance.items.set(instance.source.map((item, index) => index === 1 ? { ...item, status: "done" } : item));
  });
}

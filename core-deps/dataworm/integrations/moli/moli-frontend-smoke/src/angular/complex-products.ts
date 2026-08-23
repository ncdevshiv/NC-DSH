import { Component, signal, type Type } from "@angular/core";

import { deterministicWords, money, stableItems } from "../shared/data";
import type { CaseSpec, SmokeMeta } from "../shared/types";
import { bootstrapAngular } from "./support";

interface ProductComponent {
  selected: ReturnType<typeof signal<number>>;
  mode: ReturnType<typeof signal<string>>;
}

function componentType(meta: SmokeMeta, spec: CaseSpec): Type<ProductComponent> {
  class AngularProductComponent implements ProductComponent {
    readonly meta = meta;
    readonly spec = spec;
    readonly items = stableItems(spec.seed, 36);
    readonly selected = signal(0);
    readonly mode = signal("loading");
    readonly statuses = ["new", "active", "paused", "done"] as const;
    readonly deterministicWords = deterministicWords;
    readonly money = money;
    readonly String = String;
    readonly Math = Math;
    byStatus(status: string) { return this.items.filter((item) => item.status === status); }
  }
  Component({
    selector: "smoke-root",
    standalone: true,
    template: `
      <main id="smoke-root" data-framework="angular" [attr.data-family]="meta.family" [attr.data-mode]="mode()">
        <header class="product-header"><a href="#home">Moli Suite</a><nav><a href="#work">Work</a><a href="#reports">Reports</a><a href="#team">Team</a></nav><button>Profile</button></header>
        <h1>{{ meta.title }}</h1><section data-case-body>
          @switch (spec.variant) {
            @case (0) { <section class="hero"><h2>Operations overview</h2><p>{{ deterministicWords(spec.seed, 30) }}</p></section><div class="metrics">@for (item of items.slice(0, 6); track item.id) { <article><span>{{ item.status }}</span><strong>{{ item.amount }}</strong><small>{{ item.title }}</small></article> }</div><table><tbody>@for (item of items.slice(6, 22); track item.id) { <tr><td>{{ item.title }}</td><td>{{ item.owner }}</td><td>{{ item.status }}</td><td>{{ money(item.amount) }}</td></tr> }</tbody></table> }
            @case (1) { <aside><h2>Categories</h2>@for (name of ["All", "Runtime", "DOM", "Network", "Storage"]; track name; let index = $index) { <button [attr.aria-pressed]="selected() === index">{{ name }}</button> }</aside><section class="catalog">@for (item of items.slice(0, 20); track item.id) { <article><div class="product-image" [attr.aria-label]="item.title">{{ item.title.slice(0, 2) }}</div><h3>{{ item.title }}</h3><p>{{ deterministicWords(item.amount, 12) }}</p><strong>{{ money(item.amount) }}</strong><button>Add</button></article> }</section> }
            @case (2) { <div class="kanban">@for (status of statuses; track status) { <section><header><h2>{{ status }}</h2><span>{{ byStatus(status).length }}</span></header>@for (item of byStatus(status); track item.id) { <article><h3>{{ item.title }}</h3><p>{{ item.owner }}</p><div>@for (tag of item.tags; track tag) { <span>{{ tag }}</span> }</div></article> }</section> }</div> }
            @case (3) { <div class="mail"><aside><button>Compose</button>@for (name of ["Inbox", "Starred", "Sent", "Drafts", "Archive"]; track name) { <a [href]="'#' + name">{{ name }}</a> }</aside><section class="messages">@for (item of items.slice(0, 16); track item.id; let index = $index) { <article [attr.aria-current]="selected() === index"><strong>{{ item.owner }}</strong><h3>{{ item.title }}</h3><p>{{ deterministicWords(item.amount, 14) }}</p><time>Jul {{ index + 1 }}</time></article> }</section><article class="message-detail"><h2>{{ items[selected()].title }}</h2><p>From {{ items[selected()].owner }}</p><p>{{ deterministicWords(spec.seed, 90) }}</p></article></div> }
            @case (4) { <div class="chat"><aside>@for (item of items.slice(0, 10); track item.id; let index = $index) { <button [attr.aria-current]="selected() === index"><span>{{ item.owner.slice(0, 1) }}</span>{{ item.owner }}<small>{{ item.status }}</small></button> }</aside><section><header><h2>Compatibility team</h2><span>8 members</span></header><ol>@for (item of items.slice(8, 28); track item.id; let index = $index) { <li [attr.data-own]="index % 4 === 0"><strong>{{ item.owner }}</strong><p>{{ item.title }}: {{ deterministicWords(item.amount, 9) }}</p><time>10:{{ String(index * 2).padStart(2, "0") }}</time></li> }</ol><form><input value="Draft message"><button>Send</button></form></section></div> }
            @case (5) { <div class="docs"><aside><h2>Documentation</h2>@for (item of items.slice(0, 14); track item.id) { <a [href]="'#' + item.id">{{ item.title }}</a> }</aside><article><p class="breadcrumbs">Docs / Runtime / Lifecycle</p><h2>Renderer lifecycle</h2>@for (item of items.slice(0, 9); track item.id; let index = $index) { <section><h3 [id]="'section-' + index">{{ index + 1 }}. {{ item.title }}</h3><p>{{ deterministicWords(spec.seed + index, 42) }}</p>@if (index % 3 === 0) { <pre><code>const phase{{ index }} = "ready";</code></pre> }</section> }</article><nav aria-label="On this page">@for (item of items.slice(0, 9); track item.id; let index = $index) { <a [href]="'#section-' + index">{{ item.title }}</a> }</nav></div> }
            @case (6) { <div class="booking"><section><h2>Find a workspace</h2><form><label>Location <input value="Shanghai"></label><label>Date <input type="date" value="2026-08-12"></label><label>People <input type="number" value="4"></label><button>Search</button></form></section><section class="results">@for (item of items.slice(0, 14); track item.id; let index = $index) { <article><div [attr.aria-label]="'Room preview ' + (index + 1)">{{ index + 1 }}</div><h3>{{ item.title }} Room</h3><p>{{ item.tags.join(" · ") }}</p><strong>{{ money(item.amount) }} / day</strong><button>Select</button></article> }</section><aside><h2>Your booking</h2><p>{{ items[selected()].title }}</p><dl><dt>Subtotal</dt><dd>{{ money(items[selected()].amount) }}</dd><dt>Service</dt><dd>$12.00</dd></dl></aside></div> }
            @case (7) { <div class="issues"><aside><h2>Repositories</h2>@for (item of items.slice(0, 9); track item.id) { <a [href]="'#' + item.id">{{ item.tags[0] }}/{{ item.title }}</a> }</aside><section><header><h2>Issues</h2><button>New issue</button></header><div class="filters">@for (name of ["Open", "Assigned", "Mentioned", "Closed"]; track name; let index = $index) { <button [attr.aria-pressed]="selected() === index">{{ name }}</button> }</div>@for (item of items.slice(0, 24); track item.id; let index = $index) { <article><span [attr.aria-label]="item.status">●</span><div><h3>#{{ 1200 + index }} {{ item.title }}</h3><p>opened by {{ item.owner }} · {{ item.tags.join(", ") }}</p></div><strong>{{ index % 5 }}</strong></article> }</section></div> }
            @case (8) { <div class="analytics"><section class="toolbar"><h2>Traffic analytics</h2><select value="30"><option value="30">Last 30 days</option></select></section><div class="kpis">@for (item of items.slice(0, 5); track item.id) { <article><span>{{ item.title }}</span><strong>{{ item.amount * 13 }}</strong><small>+{{ item.amount % 27 }}%</small></article> }</div><section class="chart" aria-label="Traffic bars">@for (item of items.slice(5, 25); track item.id) { <div [style.height.px]="20 + item.amount % 80"><span>{{ item.amount }}</span></div> }</section><table><thead><tr><th>Page</th><th>Views</th><th>Users</th><th>Rate</th></tr></thead><tbody>@for (item of items.slice(10, 28); track item.id) { <tr><td>/{{ item.tags[0] }}/{{ item.id }}</td><td>{{ item.amount }}</td><td>{{ Math.floor(item.amount / 3) }}</td><td>{{ item.amount % 100 }}%</td></tr> }</tbody></table></div> }
            @default { <div class="invoice"><header><div><h2>INVOICE</h2><p>#MOLI-{{ spec.seed }}</p></div><address>Moli Browser Labs<br>88 Runtime Road<br>Shanghai</address></header><section class="invoice-meta"><dl><dt>Issued</dt><dd>2026-07-30</dd><dt>Due</dt><dd>2026-08-30</dd><dt>Status</dt><dd>Draft</dd></dl><address>Bill to<br><strong>Chromium Reference</strong><br>Automation Team</address></section><table><thead><tr><th>Description</th><th>Qty</th><th>Rate</th><th>Amount</th></tr></thead><tbody>@for (item of items.slice(0, 18); track item.id; let index = $index) { <tr><td>{{ item.title }}<small>{{ item.tags.join(", ") }}</small></td><td>{{ index % 4 + 1 }}</td><td>{{ money(item.amount) }}</td><td>{{ money(item.amount * (index % 4 + 1)) }}</td></tr> }</tbody></table><footer><dl><dt>Subtotal</dt><dd>$18,420.00</dd><dt>Tax</dt><dd>$1,842.00</dd><dt>Total</dt><dd>$20,262.00</dd></dl><p>{{ deterministicWords(spec.seed, 32) }}</p></footer></div> }
          }
        </section>
      </main>
    `,
  })(AngularProductComponent);
  return AngularProductComponent;
}

export async function mount(meta: SmokeMeta, spec: CaseSpec): Promise<void> {
  await bootstrapAngular(componentType(meta, spec), meta, "angular-complex-product-ready", (instance) => {
    instance.selected.set((spec.seed + spec.variant) % 7);
    instance.mode.set("ready");
  });
}

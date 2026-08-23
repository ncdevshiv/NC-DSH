import { Component, computed, signal, type Type } from "@angular/core";

import { deterministicWords, money, stableItems } from "../shared/data";
import type { CaseSpec, SmokeMeta } from "../shared/types";
import { bootstrapAngular } from "./support";

interface AppComponent {
  filter: ReturnType<typeof signal<string>>;
  page: ReturnType<typeof signal<number>>;
}

function componentType(meta: SmokeMeta, spec: CaseSpec): Type<AppComponent> {
  class AngularAppComponent implements AppComponent {
    readonly meta = meta;
    readonly spec = spec;
    readonly allItems = stableItems(spec.seed, 24);
    readonly filter = signal("all");
    readonly page = signal(0);
    readonly visible = computed(() => {
      const selected = this.allItems.filter((item) => this.filter() === "all" || item.status === this.filter());
      return selected.length ? selected : this.allItems.slice(0, 6);
    });
    readonly deterministicWords = deterministicWords;
    readonly money = money;
    readonly String = String;
  }
  Component({
    selector: "smoke-root",
    standalone: true,
    template: `
      <main id="smoke-root" data-framework="angular" [attr.data-family]="meta.family" [attr.data-filter]="filter()">
        <h1>{{ meta.title }}</h1><section data-case-body>
          @switch (spec.variant) {
            @case (0) { <header><a class="brand" href="#home">Moli</a><nav aria-label="Primary"><a href="#overview">Overview</a><a href="#activity">Activity</a><a href="#settings">Settings</a></nav><button>Account</button></header><section><h2>Welcome</h2><p>{{ deterministicWords(spec.seed, 24) }}</p></section> }
            @case (1) { <div class="sidebar-layout"><aside><h2>Workspace</h2>@for (item of allItems.slice(0, 8); track item.id) { <a [href]="'#' + item.id">{{ item.title }}</a> }</aside><section><h2>Selected view</h2>{{ deterministicWords(spec.seed, 40) }}</section></div> }
            @case (2) { <div class="stats-grid">@for (item of allItems.slice(0, 8); track item.id; let index = $index) { <article><h2>{{ item.title }}</h2><strong>{{ item.amount + index * 17 }}</strong><small>{{ item.status }}</small></article> }</div> }
            @case (3) { <label>Search <input [value]="filter()" readonly></label><table><thead><tr><th>Name</th><th>Owner</th><th>Status</th></tr></thead><tbody>@for (item of visible(); track item.id) { <tr><td>{{ item.title }}</td><td>{{ item.owner }}</td><td>{{ item.status }}</td></tr> }</tbody></table> }
            @case (4) { <section>@for (item of allItems.slice(page() * 6, page() * 6 + 6); track item.id) { <article><h2>{{ item.title }}</h2></article> }</section><nav aria-label="Pages">@for (value of [1, 2, 3, 4]; track value) { <button [attr.aria-current]="page() + 1 === value ? 'page' : null">{{ value }}</button> }</nav> }
            @case (5) { <div class="chips">@for (value of ["all", "new", "active", "done"]; track value) { <button [attr.aria-pressed]="filter() === value">{{ value }}</button> }</div><ul>@for (item of visible(); track item.id) { <li>{{ item.title }}</li> }</ul> }
            @case (6) { <article class="profile"><header><div aria-hidden="true">ML</div><div><h2>Moli Light</h2><p>Browser runtime engineer</p></div></header><dl><dt>Projects</dt><dd>18</dd><dt>Open reviews</dt><dd>7</dd><dt>Compatibility</dt><dd>92%</dd></dl><section><h3>About</h3><p>{{ deterministicWords(spec.seed, 60) }}</p></section></article> }
            @case (7) { <section class="notifications"><header><h2>Notifications</h2><button>Mark all read</button></header>@for (item of allItems.slice(0, 12); track item.id; let index = $index) { <article [attr.data-unread]="index < 4"><strong>{{ item.owner }}</strong><p>{{ item.title }}</p><time>09:{{ String(index * 4).padStart(2, "0") }}</time></article> }</section> }
            @case (8) { <form class="settings"><nav><button type="button">General</button><button type="button">Network</button><button type="button">Privacy</button></nav><section><h2>General settings</h2><label>Workspace name <input value="Moli Lab"></label><label>Theme <select value="system"><option value="system">System</option></select></label>@for (label of ["Tracing", "Caching", "Diagnostics"]; track label; let index = $index) { <label><input type="checkbox" [checked]="index !== 1">{{ label }}</label> }</section></form> }
            @default { <div class="admin"><header><h2>Administration</h2><button>Add member</button></header><section class="stats">@for (item of allItems.slice(0, 4); track item.id) { <article><span>{{ item.status }}</span><strong>{{ money(item.amount) }}</strong></article> }</section><table><thead><tr><th>User</th><th>Role</th><th>Status</th><th>Actions</th></tr></thead><tbody>@for (item of allItems.slice(0, 16); track item.id) { <tr><td>{{ item.owner }}</td><td>{{ item.tags[0] }}</td><td>{{ item.status }}</td><td><button>Edit</button></td></tr> }</tbody></table></div> }
          }
        </section>
      </main>
    `,
  })(AngularAppComponent);
  return AngularAppComponent;
}

export async function mount(meta: SmokeMeta, spec: CaseSpec): Promise<void> {
  await bootstrapAngular(componentType(meta, spec), meta, "angular-composed-app-update", (instance) => {
    instance.filter.set(spec.variant % 2 === 0 ? "active" : "done");
    instance.page.set(1);
  });
}

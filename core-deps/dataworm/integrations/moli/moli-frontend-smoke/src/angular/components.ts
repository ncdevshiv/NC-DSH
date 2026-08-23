import { Component, InjectionToken, inject, signal, type Type } from "@angular/core";

import { stableItems } from "../shared/data";
import type { CaseSpec, SmokeMeta } from "../shared/types";
import { bootstrapAngular } from "./support";

const THEME = new InjectionToken<string>("smoke-theme");

class BadgeComponent {
  label = "";
}
Component({
  selector: "smoke-badge",
  standalone: true,
  inputs: ["label"],
  template: `<span class="badge">{{ label }}</span>`,
})(BadgeComponent);

interface ComponentsComponent {
  active: ReturnType<typeof signal<number>>;
}

function componentType(meta: SmokeMeta, spec: CaseSpec): Type<ComponentsComponent> {
  class AngularComponentsComponent implements ComponentsComponent {
    readonly meta = meta;
    readonly spec = spec;
    readonly active = signal(0);
    readonly items = stableItems(spec.seed, Math.max(5, spec.size));
    readonly theme = inject(THEME);
    activeLabel() { return ["Overview", "Network", "Runtime"][this.active()]; }
  }
  Component({
    selector: "smoke-root",
    standalone: true,
    imports: [BadgeComponent],
    providers: [{ provide: THEME, useValue: "ocean" }],
    template: `
      <main id="smoke-root" data-framework="angular" [attr.data-family]="meta.family">
        <h1>{{ meta.title }}</h1><section data-case-body>
          @switch (spec.variant) {
            @case (0) { <smoke-badge [label]="'Input value ' + spec.seed" /> }
            @case (1) { <article><h2>Parent</h2><section><h3>Child</h3><smoke-badge label="Grandchild" /></section></article> }
            @case (2) { <article [attr.data-theme]="theme"><h2>Injection consumer</h2></article> }
            @case (3) { <section><header><smoke-badge label="Header projection" /></header><main>@for (item of items.slice(0, 3); track item.id) { <p>{{ item.title }}</p> }</main><footer>Footer projection</footer></section> }
            @case (4) { @if (active() === 1) { <article data-component="alpha">Alpha component</article> } @else { <aside data-component="beta">Beta component</aside> } }
            @case (5) { <ul><li data-depth="0">{{ items[0].title }}<ul><li data-depth="1">{{ items[1].title }}<ul><li data-depth="2">{{ items[2].title }}<ul><li data-depth="3">{{ items[3].title }}</li></ul></li></ul></li></ul></li></ul> }
            @case (6) { <section><p>Local content</p><aside id="portal-content">Dynamic host {{ spec.seed }}</aside></section> }
            @case (7) { <div role="group"><smoke-badge label="Prefix" /><button>Action</button><smoke-badge label="Suffix" /></div> }
            @case (8) { <section><div role="tablist">@for (label of ["Overview", "Network", "Runtime"]; track label; let index = $index) { <button role="tab" [attr.aria-selected]="active() === index">{{ label }}</button> }</div><article role="tabpanel"><h2>{{ activeLabel() }}</h2><p>Selected panel {{ active() }}</p></article></section> }
            @default { <div class="app-shell"><header><h2>Moli Console</h2><smoke-badge label="online" /></header><nav>@for (item of items.slice(0, 5); track item.id) { <a [href]="'#' + item.id">{{ item.title }}</a> }</nav><section>@for (item of items.slice(5); track item.id) { <article><h3>{{ item.title }}</h3><p>{{ item.owner }}</p></article> }</section><footer>Build {{ spec.seed }}</footer></div> }
          }
        </section>
      </main>
    `,
  })(AngularComponentsComponent);
  return AngularComponentsComponent;
}

export async function mount(meta: SmokeMeta, spec: CaseSpec): Promise<void> {
  await bootstrapAngular(componentType(meta, spec), meta, "angular-component-composition", (instance) => {
    instance.active.set((spec.variant + 1) % 3);
  });
}

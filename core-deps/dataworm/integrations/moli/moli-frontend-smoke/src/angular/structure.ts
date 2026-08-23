import { Component, signal, type Type } from "@angular/core";

import { deterministicWords } from "../shared/data";
import type { CaseSpec, SmokeMeta } from "../shared/types";
import { bootstrapAngular } from "./support";

interface StructureComponent {
  expanded: ReturnType<typeof signal<boolean>>;
}

function componentType(meta: SmokeMeta, spec: CaseSpec): Type<StructureComponent> {
  class AngularStructureComponent implements StructureComponent {
    readonly meta = meta;
    readonly spec = spec;
    readonly expanded = signal(false);
    readonly sections = Array.from({ length: 12 }, (_, index) => ({
      id: `section-${index}`,
      title: `Section ${index + 1}`,
      text: deterministicWords(spec.seed + index, 18),
    }));
    readonly words = deterministicWords(spec.seed, 12);
    readonly String = String;
  }
  Component({
    selector: "smoke-root",
    standalone: true,
    template: `
      <main id="smoke-root" data-framework="angular" [attr.data-family]="meta.family"
        [attr.data-expanded]="String(expanded())">
        <h1>{{ meta.title }}</h1>
        <section data-case-body>
          @switch (spec.variant) {
            @case (0) { @if (expanded()) { <p data-branch="present">The conditional branch is present.</p> } }
            @case (1) { @if (expanded()) { <aside data-mode="expanded">Expanded details</aside> } @else { <p>Compact</p> } }
            @case (2) { <span>Alpha</span><b>Beta</b><i>Gamma</i><span>Delta</span> }
            @case (3) { <template data-kind="card"><article><h2>Template content</h2><p>Retained subtree</p></article></template> }
            @case (4) { <!--angular-marker--><span>After marker</span> }
            @case (5) { <details [open]="expanded()"><summary>Compatibility details</summary><p>{{ words }}</p></details> }
            @case (6) { <dl><div><dt>Engine</dt><dd>Moli</dd></div><div><dt>Reference</dt><dd>Chromium</dd></div></dl> }
            @case (7) { <article><header><h2>Semantic article</h2></header><nav aria-label="Article"><a href="#intro">Intro</a></nav><section id="intro"><p>Body</p></section><footer>End</footer></article> }
            @case (8) { <div data-depth="14"><div data-depth="13"><div data-depth="12"><div data-depth="11"><div data-depth="10"><div data-depth="9"><div data-depth="8"><div data-depth="7"><div data-depth="6"><div data-depth="5"><div data-depth="4"><div data-depth="3"><div data-depth="2"><div data-depth="1"><strong id="deep-leaf">Deep leaf {{ spec.seed }}</strong></div></div></div></div></div></div></div></div></div></div></div></div></div></div> }
            @default { @for (section of sections; track section.id) { <section [attr.aria-labelledby]="section.id"><h2 [id]="section.id">{{ section.title }}</h2><p>{{ section.text }}</p></section> } }
          }
        </section>
      </main>
    `,
  })(AngularStructureComponent);
  return AngularStructureComponent;
}

export async function mount(meta: SmokeMeta, spec: CaseSpec): Promise<void> {
  await bootstrapAngular(componentType(meta, spec), meta, "angular-structure-committed", (instance) => {
    instance.expanded.set(true);
  });
}

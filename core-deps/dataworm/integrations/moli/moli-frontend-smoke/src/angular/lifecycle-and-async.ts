import { Component, signal, type Type } from "@angular/core";

import { captureFrame } from "../shared/harness";
import { stableItems } from "../shared/data";
import type { CaseSpec, SmokeMeta } from "../shared/types";
import { bootstrapAngular, settleAngular } from "./support";

interface AsyncComponent {
  phase: ReturnType<typeof signal<string>>;
  showChild: ReturnType<typeof signal<boolean>>;
  timeline: ReturnType<typeof signal<string[]>>;
  update(label: string): void;
}

function componentType(meta: SmokeMeta, spec: CaseSpec): Type<AsyncComponent> {
  class AngularAsyncComponent implements AsyncComponent {
    readonly meta = meta;
    readonly spec = spec;
    readonly phase = signal("mounted");
    readonly showChild = signal(true);
    readonly timeline = signal(["render"]);
    readonly items = stableItems(spec.seed, spec.variant >= 8 ? 24 : 5);
    readonly stages = Array.from({ length: 12 }, (_, index) => index);
    update(label: string) {
      this.phase.set(label);
      this.timeline.update((current) => [...current, label]);
      if (spec.variant === 7) this.showChild.set(false);
    }
  }
  Component({
    selector: "smoke-root",
    standalone: true,
    template: `
      <main id="smoke-root" data-framework="angular" [attr.data-family]="meta.family" [attr.data-phase]="phase()">
        <h1>{{ meta.title }}</h1><section data-case-body><p role="status">Phase: {{ phase() }}</p>
          @if (showChild()) { <aside id="async-child">Child {{ spec.seed }}</aside> }
          @if (spec.variant >= 5) { <ul>@for (item of items; track item.id) { <li>{{ item.title }} — {{ phase() }}</li> }</ul> }
          @if (spec.variant >= 6) { <ol>@for (entry of timeline(); track $index; let index = $index) { <li>{{ index }}: {{ entry }}</li> }</ol> }
          @if (spec.variant === 9) { <div class="timeline">@for (index of stages; track index) { <article><h2>Stage {{ index + 1 }}</h2><p>{{ timeline().join(" → ") }}</p></article> }</div> }
        </section>
      </main>
    `,
  })(AngularAsyncComponent);
  return AngularAsyncComponent;
}

export async function mount(meta: SmokeMeta, spec: CaseSpec): Promise<void> {
  await bootstrapAngular(componentType(meta, spec), meta, "angular-async-settled", async (instance, application) => {
    if (spec.variant === 9) {
      for (let index = 1; index <= 3; index += 1) {
        await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
        instance.update(`frame-${index}`);
        await settleAngular(application);
        await captureFrame(meta, `animation-frame-${index}`);
      }
      return;
    }
    const label = spec.variant === 4 ? "timer" : spec.variant === 2 ? "microtask" : spec.variant >= 3 ? "promise" : "effect";
    if (spec.variant === 4) await new Promise<void>((resolve) => setTimeout(resolve, 0));
    else await Promise.resolve();
    instance.update(label);
  });
}

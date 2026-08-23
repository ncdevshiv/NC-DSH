import { Component, signal, type Type } from "@angular/core";

import {
  runBrowsingContextBoundaryCase,
  type BrowsingContextBoundaryResult,
} from "../shared/browsing-context-boundaries";
import { assertFixture, captureFrame } from "../shared/harness";
import type { CaseSpec, SmokeMeta } from "../shared/types";
import { bootstrapAngular } from "./support";

interface BrowsingContextBoundaryComponent {
  result: ReturnType<typeof signal<BrowsingContextBoundaryResult | undefined>>;
  run(): Promise<void>;
}

function componentType(meta: SmokeMeta, spec: CaseSpec): Type<BrowsingContextBoundaryComponent> {
  class AngularBrowsingContextBoundaryComponent implements BrowsingContextBoundaryComponent {
    readonly meta = meta;
    readonly result = signal<BrowsingContextBoundaryResult | undefined>(undefined);

    async run(): Promise<void> {
      const host = document.querySelector("[data-boundary-host]");
      assertFixture(host instanceof HTMLElement, "Angular browsing-context host exists");
      this.result.set(
        await runBrowsingContextBoundaryCase(host, meta, spec, (name) =>
          captureFrame(meta, name),
        ),
      );
    }
  }

  Component({
    selector: "smoke-root",
    standalone: true,
    template: `
      <main
        id="smoke-root"
        data-framework="angular"
        [attr.data-family]="meta.family"
        [attr.data-mode]="result()?.status ?? 'loading'"
      >
        <h1>{{ meta.title }}</h1>
        <section data-case-body>
          <div data-boundary-host></div>
          @if (result(); as current) {
            <dl data-boundary-facts>
              @for (item of current.facts; track item.name) {
                <div [attr.data-fact]="item.name">
                  <dt>{{ item.name }}</dt>
                  <dd>{{ item.value }}</dd>
                </div>
              }
            </dl>
          }
        </section>
      </main>
    `,
  })(AngularBrowsingContextBoundaryComponent);
  return AngularBrowsingContextBoundaryComponent;
}

export async function mount(meta: SmokeMeta, spec: CaseSpec): Promise<void> {
  await bootstrapAngular(
    componentType(meta, spec),
    meta,
    "angular-browsing-context-ready",
    (instance) => instance.run(),
  );
}

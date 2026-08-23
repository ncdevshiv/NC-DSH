import { Component, signal, type Type } from "@angular/core";

import { assertFixture } from "../shared/harness";
import type { CaseSpec, SmokeMeta } from "../shared/types";
import {
  runWebPlatformIntegrationCase,
  type WebPlatformIntegrationResult,
} from "../shared/web-platform-integration";
import { bootstrapAngular } from "./support";

interface WebPlatformComponent {
  result: ReturnType<typeof signal<WebPlatformIntegrationResult | undefined>>;
  run(): Promise<void>;
}

function componentType(meta: SmokeMeta, spec: CaseSpec): Type<WebPlatformComponent> {
  class AngularWebPlatformComponent implements WebPlatformComponent {
    readonly meta = meta;
    readonly result = signal<WebPlatformIntegrationResult | undefined>(undefined);

    async run(): Promise<void> {
      const host = document.querySelector("[data-platform-host]");
      assertFixture(host instanceof HTMLElement, "Angular web-platform host exists");
      this.result.set(await runWebPlatformIntegrationCase(host, meta, spec));
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
          <div data-platform-host></div>
          @if (result(); as current) {
            <dl data-platform-facts>
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
  })(AngularWebPlatformComponent);
  return AngularWebPlatformComponent;
}

export async function mount(meta: SmokeMeta, spec: CaseSpec): Promise<void> {
  await bootstrapAngular(
    componentType(meta, spec),
    meta,
    "angular-web-platform-ready",
    (instance) => instance.run(),
  );
}

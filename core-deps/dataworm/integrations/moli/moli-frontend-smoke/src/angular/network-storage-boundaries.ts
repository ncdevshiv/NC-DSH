import { Component, signal, type Type } from "@angular/core";

import { assertFixture, captureFrame } from "../shared/harness";
import {
  runNetworkStorageBoundaryCase,
  type NetworkStorageBoundaryResult,
} from "../shared/network-storage-boundaries";
import type { CaseSpec, SmokeMeta } from "../shared/types";
import { bootstrapAngular } from "./support";

interface NetworkStorageBoundaryComponent {
  result: ReturnType<typeof signal<NetworkStorageBoundaryResult | undefined>>;
  run(): Promise<void>;
}

function componentType(meta: SmokeMeta, spec: CaseSpec): Type<NetworkStorageBoundaryComponent> {
  class AngularNetworkStorageBoundaryComponent implements NetworkStorageBoundaryComponent {
    readonly meta = meta;
    readonly result = signal<NetworkStorageBoundaryResult | undefined>(undefined);

    async run(): Promise<void> {
      const host = document.querySelector("[data-platform-host]");
      assertFixture(host instanceof HTMLElement, "Angular network/storage host exists");
      this.result.set(
        await runNetworkStorageBoundaryCase(host, meta, spec, (name) =>
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
  })(AngularNetworkStorageBoundaryComponent);
  return AngularNetworkStorageBoundaryComponent;
}

export async function mount(meta: SmokeMeta, spec: CaseSpec): Promise<void> {
  await bootstrapAngular(
    componentType(meta, spec),
    meta,
    "angular-network-storage-ready",
    (instance) => instance.run(),
  );
}

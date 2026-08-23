import "@angular/compiler";

import { CommonModule } from "@angular/common";
import {
  ApplicationRef,
  Component,
  signal,
  type Type,
  provideZonelessChangeDetection,
} from "@angular/core";
import { bootstrapApplication } from "@angular/platform-browser";

import { deterministicWords, stableItems, type StableItem } from "../shared/data";
import { assertFixture, captureFrame, markReady } from "../shared/harness";
import type { CaseSpec, SmokeMeta } from "../shared/types";

interface CaseComponent {
  meta: SmokeMeta;
  spec: CaseSpec;
  updated: ReturnType<typeof signal<boolean>>;
  items: StableItem[];
  words: string;
  literalMarkup: string;
  String: StringConstructor;
}

function componentType(meta: SmokeMeta, spec: CaseSpec): Type<CaseComponent> {
  class TextAndAttributesComponent implements CaseComponent {
    readonly meta = meta;
    readonly spec = spec;
    readonly updated = signal(false);
    readonly items = stableItems(spec.seed, Math.min(spec.size, 12));
    readonly words = deterministicWords(spec.seed, 28);
    readonly literalMarkup = "<strong>literal & safe</strong> \"quoted\" 'single'";
    readonly String = String;
  }

  Component({
    selector: "smoke-root",
    standalone: true,
    imports: [CommonModule],
    template: `
      <main id="smoke-root" data-framework="angular" [attr.data-family]="meta.family"
        [attr.data-updated]="String(updated())">
        <h1>{{ meta.title }}</h1>
        <section data-case-body>
          @switch (spec.variant) {
            @case (0) {
              <p [attr.data-value]="spec.seed">Hello {{ spec.title }}; value {{ spec.seed * 3 }}</p>
            }
            @case (1) {
              <p>{{ literalMarkup }}</p>
            }
            @case (2) {
              <input type="text" aria-label="Boolean projection"
                [disabled]="!updated()" [hidden]="false" [required]="updated()">
            }
            @case (3) {
              <section [attr.aria-label]="'Panel ' + spec.seed" [attr.aria-busy]="!updated()"
                [attr.data-seed]="spec.seed">ARIA dataset</section>
            }
            @case (4) {
              <div [class]="updated() ? 'card active selected' : 'card pending'">Class tokens</div>
            }
            @case (5) {
              <div [style.color]="updated() ? 'rgb(12, 34, 56)' : 'black'"
                [style.margin-top.px]="spec.variant + 2">Style map</div>
            }
            @case (6) {
              <p lang="zh-Hans" dir="auto">你好，世界 — مرحبا — café — 😀 — {{ updated() ? "更新" : "初始" }}</p>
            }
            @case (7) {
              <dl><dt>Present</dt><dd>{{ updated() ? "value" : null }}</dd>
                <dt>Missing</dt><dd>{{ undefined }}</dd></dl>
            }
            @case (8) {
              <dl>
                @for (item of items; track item.id) {
                  <div><dt>{{ item.title }}</dt><dd [attr.data-status]="item.status">{{ item.owner }}</dd></div>
                }
              </dl>
            }
            @default {
              <article>
                <header><p class="eyebrow">Engineering / Browser</p><h2>{{ spec.title }}</h2>
                  <p>{{ words }}</p></header>
                <div>
                  @for (item of items; track item.id) {
                    <span [attr.data-tag]="item.tags[0]">{{ item.title }}</span>
                  }
                </div>
              </article>
            }
          }
        </section>
      </main>
    `,
  })(TextAndAttributesComponent);

  return TextAndAttributesComponent;
}

async function settleAngular(application: ApplicationRef): Promise<void> {
  await application.whenStable();
  await Promise.resolve();
  await application.whenStable();
}

export async function mount(meta: SmokeMeta, spec: CaseSpec): Promise<void> {
  const application = await bootstrapApplication(componentType(meta, spec), {
    providers: [provideZonelessChangeDetection()],
  });
  await settleAngular(application);
  const instance = application.components[0]?.instance as CaseComponent | undefined;
  assertFixture(instance, "Angular root component exists");
  assertFixture(document.querySelector("[data-case-body]") !== null, "case body mounted");
  await captureFrame(meta, "mounted");
  instance.updated.set(true);
  await settleAngular(application);
  markReady(meta, ["mounted", "angular-stable-update"]);
}

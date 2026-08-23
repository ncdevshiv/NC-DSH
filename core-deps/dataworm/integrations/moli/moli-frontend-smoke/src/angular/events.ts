import { Component, signal, type Type } from "@angular/core";

import { assertFixture } from "../shared/harness";
import type { CaseSpec, SmokeMeta } from "../shared/types";
import { bootstrapAngular } from "./support";

interface EventsComponent {
  log: ReturnType<typeof signal<string[]>>;
}

function componentType(meta: SmokeMeta, spec: CaseSpec): Type<EventsComponent> {
  class AngularEventsComponent implements EventsComponent {
    readonly meta = meta;
    readonly spec = spec;
    readonly log = signal<string[]>([]);
    append(value: string) { this.log.update((current) => [...current, value]); }
    input(event: Event) { this.append(`input:${(event.currentTarget as HTMLInputElement).value}`); }
    change(event: Event) { this.append(`change:${(event.currentTarget as HTMLSelectElement).value}`); }
    submit(event: Event) { event.preventDefault(); this.append("submit:prevented"); }
    keydown(event: KeyboardEvent) { this.append(`key:${event.key}:${event.ctrlKey}`); }
  }
  Component({
    selector: "smoke-root",
    standalone: true,
    template: `
      <main id="smoke-root" data-framework="angular" [attr.data-family]="meta.family">
        <h1>{{ meta.title }}</h1><section data-case-body>
          @switch (spec.variant) {
            @case (3) { <input data-trigger value="initial" (input)="input($event)"> }
            @case (4) { <select data-trigger value="new" (change)="change($event)"><option value="new">New</option><option value="done">Done</option></select> }
            @case (5) { <form data-trigger (submit)="submit($event)"><button>Submit</button></form> }
            @case (6) { <input data-trigger aria-label="Command" (keydown)="keydown($event)"> }
            @case (9) { <input data-trigger aria-label="Command" (keydown)="keydown($event)"> }
            @case (1) { <div (click)="append('parent')"><button data-trigger (click)="append('child')">Bubble</button></div> }
            @case (2) { <div (click)="append('parent')"><button data-trigger (click)="$event.stopPropagation(); append('child-stopped')">Stop</button></div> }
            @case (7) { <button data-trigger (click)="append('output:' + spec.seed)">Emit output</button> }
            @case (8) { <button data-trigger (click)="append('first'); append('second'); append('third')">Handlers</button> }
            @default { <button data-trigger (click)="append('click:updated')">Click</button> }
          }
          <output><ol>@for (entry of log(); track $index) { <li>{{ entry }}</li> }</ol></output>
        </section>
      </main>
    `,
  })(AngularEventsComponent);
  return AngularEventsComponent;
}

export async function mount(meta: SmokeMeta, spec: CaseSpec): Promise<void> {
  await bootstrapAngular(componentType(meta, spec), meta, "angular-template-event", (instance) => {
    const trigger = document.querySelector<HTMLElement>("[data-trigger]");
    assertFixture(trigger, "Angular event trigger mounted");
    if (spec.variant === 3) {
      (trigger as HTMLInputElement).value = `typed-${spec.seed}`;
      trigger.dispatchEvent(new InputEvent("input", { bubbles: true, data: "x" }));
    } else if (spec.variant === 4) {
      (trigger as HTMLSelectElement).value = "done";
      trigger.dispatchEvent(new Event("change", { bubbles: true }));
    } else if (spec.variant === 5) {
      trigger.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    } else if (spec.variant === 6 || spec.variant === 9) {
      trigger.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: spec.variant === 9 ? "k" : "Enter", ctrlKey: spec.variant === 9 }));
    } else trigger.click();
    assertFixture(instance.log().length > 0, "Angular event produced a log entry");
  });
}

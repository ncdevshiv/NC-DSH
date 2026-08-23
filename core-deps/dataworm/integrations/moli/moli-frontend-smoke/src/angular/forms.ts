import { Component, signal, type Type } from "@angular/core";
import { FormsModule } from "@angular/forms";

import type { CaseSpec, SmokeMeta } from "../shared/types";
import { bootstrapAngular } from "./support";

interface FormsComponent {
  name: ReturnType<typeof signal<string>>;
  notes: ReturnType<typeof signal<string>>;
  checked: ReturnType<typeof signal<string[]>>;
  priority: ReturnType<typeof signal<string>>;
  status: ReturnType<typeof signal<string>>;
  regions: ReturnType<typeof signal<string[]>>;
}

function componentType(meta: SmokeMeta, spec: CaseSpec): Type<FormsComponent> {
  class AngularFormsComponent implements FormsComponent {
    readonly meta = meta;
    readonly spec = spec;
    readonly name = signal("Initial");
    readonly notes = signal("Draft");
    readonly checked = signal(["dom"]);
    readonly priority = signal("normal");
    readonly status = signal("new");
    readonly regions = signal(["eu"]);
    has(value: string) { return this.checked().includes(value); }
    region(value: string) { return this.regions().includes(value); }
    email() { return `${this.name().replace(" ", ".").toLowerCase()}@example.test`; }
  }
  Component({
    selector: "smoke-root",
    standalone: true,
    imports: [FormsModule],
    template: `
      <main id="smoke-root" data-framework="angular" [attr.data-family]="meta.family">
        <h1>{{ meta.title }}</h1><section data-case-body>
          @switch (spec.variant) {
            @case (0) { <label>Name <input [ngModel]="name()" readonly></label> }
            @case (1) { <label>Notes <textarea [ngModel]="notes()" readonly></textarea></label> }
            @case (2) { <fieldset><legend>Scopes</legend>@for (value of ["dom", "runtime", "network"]; track value) { <label><input type="checkbox" [value]="value" [ngModel]="has(value)">{{ value }}</label> }</fieldset> }
            @case (3) { <fieldset><legend>Priority</legend>@for (value of ["low", "normal", "high"]; track value) { <label><input type="radio" name="priority" [value]="value" [ngModel]="priority()">{{ value }}</label> }</fieldset> }
            @case (4) { <label>Status <select [ngModel]="status()"><option value="new">New</option><option value="active">Active</option><option value="done">Done</option></select></label> }
            @case (5) { <label>Regions <select multiple [ngModel]="regions()">@for (value of ["eu", "us", "apac"]; track value) { <option [value]="value">{{ value }}</option> }</select></label> }
            @case (6) { <div><input [value]="name()" readonly><input value="locked" disabled><button disabled>Save</button></div> }
            @case (7) { <label>Email <input type="email" [value]="email()" readonly [attr.aria-invalid]="false"><small role="status">Valid address</small></label> }
            @case (8) { <fieldset><legend>Profile editor</legend><label>Name <input [ngModel]="name()"></label><label>Notes <textarea [ngModel]="notes()"></textarea></label><label>Status <select [ngModel]="status()"><option value="done">Done</option></select></label></fieldset> }
            @default { <form><h2>Checkout</h2><label>Customer <input name="customer" [ngModel]="name()"></label><label>Delivery <select name="delivery" [ngModel]="priority()"><option value="high">Express</option></select></label><fieldset><legend>Extras</legend>@for (value of ["dom", "runtime", "network"]; track value) { <label><input type="checkbox" [name]="value" [ngModel]="has(value)">{{ value }}</label> }</fieldset><output>Total fields: 6</output><button type="submit">Place order</button></form> }
          }
        </section>
      </main>
    `,
  })(AngularFormsComponent);
  return AngularFormsComponent;
}

export async function mount(meta: SmokeMeta, spec: CaseSpec): Promise<void> {
  await bootstrapAngular(componentType(meta, spec), meta, "angular-form-signal-update", (instance) => {
    instance.name.set(`User ${spec.seed}`);
    instance.notes.set(`Updated notes ${spec.variant}`);
    instance.checked.set(["dom", "runtime"]);
    instance.priority.set("high");
    instance.status.set("done");
    instance.regions.set(["us", "apac"]);
  });
}

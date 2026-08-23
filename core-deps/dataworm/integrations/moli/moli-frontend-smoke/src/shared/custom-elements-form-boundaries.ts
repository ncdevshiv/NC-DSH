import { assertFixture, microtaskTurns } from "./harness";
import {
  capturePlatformStep,
  fact,
  type CapturePlatformFrame,
  type PlatformBoundaryResult,
  type PlatformFact,
} from "./platform-boundary";
import type { CaseSpec, SmokeMeta } from "./types";

type CustomElementScenario = (
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
) => Promise<PlatformFact[]>;

interface FormFace extends HTMLElement {
  readonly internals: ElementInternals;
}

function scenarioRoot(host: HTMLElement, name: string): HTMLElement {
  const root = document.createElement("section");
  root.dataset.customElementScenario = name;
  host.append(root);
  return root;
}

function output(root: HTMLElement, name: string, value: string): HTMLOutputElement {
  const element = document.createElement("output");
  element.dataset.customElementOutput = name;
  element.textContent = value;
  root.append(element);
  return element;
}

function elementName(spec: CaseSpec, suffix: string): string {
  return `x-${suffix}-${spec.seed}-${spec.variant}`;
}

function formDataLabel(form: HTMLFormElement): string {
  return Array.from(new FormData(form), ([name, value]) =>
    `${name}=${typeof value === "string" ? value : `${value.name}:${value.type}:${value.size}`}`,
  ).join("|");
}

async function formValueStringTransition(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const name = elementName(spec, "string-face");

  class StringValueFace extends HTMLElement implements FormFace {
    static formAssociated = true;
    readonly internals = this.attachInternals();

    setValue(value: string | null): void {
      this.internals.setFormValue(value);
      this.dataset.projectedValue = value ?? "omitted";
    }
  }
  customElements.define(name, StringValueFace);

  const form = document.createElement("form");
  form.id = `string-form-${spec.seed}`;
  const builtin = document.createElement("input");
  builtin.name = "builtin";
  builtin.value = `fixed-${spec.variant}`;
  const face = document.createElement(name) as StringValueFace;
  face.id = `face-${spec.seed}`;
  face.setAttribute("name", "custom");
  face.setValue(`alpha-${spec.seed}`);
  form.append(builtin, face);
  root.append(form);

  const first = formDataLabel(form);
  assertFixture(face.internals.form === form, "string FACE resolved its form owner");
  assertFixture(form.elements[1] === face, "string FACE participated in form.elements");
  output(root, "string-first", `${first}\nowner=${face.internals.form?.id}`);
  await capturePlatformStep(host, capture, "platform-1", "face-string-included", [
    first,
    form.elements.length,
    face.internals.form === form,
  ]);

  face.setValue(null);
  const omitted = formDataLabel(form);
  face.setValue(`beta-${spec.variant}`);
  face.setAttribute("name", "renamed");
  const second = formDataLabel(form);
  assertFixture(!omitted.includes("custom="), "null form value omitted the FACE entry");
  assertFixture(second.includes(`renamed=beta-${spec.variant}`), "renamed FACE used its latest value");
  output(root, "string-second", `${omitted}\n${second}`);
  await capturePlatformStep(host, capture, "platform-2", "face-string-replaced", [
    omitted,
    second,
    face.dataset.projectedValue,
  ]);

  return [
    fact("initial-form-data", first),
    fact("omitted-form-data", omitted),
    fact("final-form-data", second),
    fact("form-elements", form.elements.length),
    fact("owner", face.internals.form?.id ?? "null"),
  ];
}

async function formDataMultiValueTransition(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const name = elementName(spec, "formdata-face");

  class FormDataFace extends HTMLElement implements FormFace {
    static formAssociated = true;
    readonly internals = this.attachInternals();

    setEntries(entries: Array<[string, string]>, state: string): void {
      const data = new FormData();
      for (const [key, value] of entries) data.append(key, value);
      this.internals.setFormValue(data, state);
      this.dataset.entryCount = String(entries.length);
      this.dataset.state = state;
    }
  }
  customElements.define(name, FormDataFace);

  const form = document.createElement("form");
  const face = document.createElement(name) as FormDataFace;
  face.setAttribute("name", "ignored-owner-name");
  face.setEntries(
    [
      ["line", `alpha-${spec.seed}`],
      ["line", `beta-${spec.variant}`],
      ["meta", "first"],
    ],
    "draft-state",
  );
  form.append(face);
  root.append(form);

  const first = formDataLabel(form);
  assertFixture(new FormData(form).getAll("line").length === 2, "FACE FormData kept duplicate names");
  assertFixture(!first.includes("ignored-owner-name"), "FormData value supplied its own entry names");
  output(root, "formdata-first", first);
  await capturePlatformStep(host, capture, "platform-1", "face-formdata-draft", [
    first,
    face.dataset.entryCount,
    face.dataset.state,
  ]);

  face.setEntries(
    [
      ["line", `gamma-${spec.seed}`],
      ["meta", "committed"],
      ["audit", `variant-${spec.variant}`],
    ],
    "committed-state",
  );
  const second = formDataLabel(form);
  assertFixture(new FormData(form).getAll("line").length === 1, "replacing FACE FormData removed stale duplicates");
  output(root, "formdata-second", second);
  await capturePlatformStep(host, capture, "platform-2", "face-formdata-committed", [
    second,
    face.dataset.entryCount,
    face.dataset.state,
  ]);

  return [
    fact("initial", first),
    fact("final", second),
    fact("owner-name-ignored", !second.includes("ignored-owner-name")),
    fact("line-count", new FormData(form).getAll("line").length),
  ];
}

async function dynamicFormOwnerReassociation(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const name = elementName(spec, "owner-face");

  class OwnerFace extends HTMLElement implements FormFace {
    static formAssociated = true;
    readonly internals = this.attachInternals();
    readonly history: string[] = [];

    constructor() {
      super();
      this.internals.setFormValue(`owner-value-${spec.seed}`);
    }

    formAssociatedCallback(form: HTMLFormElement | null): void {
      this.history.push(form?.id ?? "null");
      this.dataset.owner = form?.id ?? "none";
    }
  }
  customElements.define(name, OwnerFace);

  const alpha = document.createElement("form");
  alpha.id = `alpha-${spec.seed}`;
  alpha.append(document.createElement("input"));
  const beta = document.createElement("form");
  beta.id = `beta-${spec.seed}`;
  const face = document.createElement(name) as OwnerFace;
  face.setAttribute("name", "owner-value");
  face.setAttribute("form", alpha.id);
  root.append(alpha, beta, face);
  await microtaskTurns(2);

  const first = `${face.internals.form?.id}|${formDataLabel(alpha)}|${formDataLabel(beta)}|${face.history.join(",")}`;
  assertFixture(face.internals.form === alpha, "FACE initially associated with alpha form");
  output(root, "owner-first", first);
  await capturePlatformStep(host, capture, "platform-1", "face-owner-alpha", [
    face.internals.form?.id,
    alpha.elements.length,
    beta.elements.length,
    face.history.join(","),
  ]);

  face.setAttribute("form", beta.id);
  beta.remove();
  root.append(beta);
  await microtaskTurns(2);
  const second = `${face.internals.form?.id}|${formDataLabel(alpha)}|${formDataLabel(beta)}|${face.history.join(",")}`;
  assertFixture(face.internals.form === beta, "FACE reassociated after beta form reinsertion");
  assertFixture(face.history.includes("null"), "form removal published a null owner callback");
  output(root, "owner-second", second);
  await capturePlatformStep(host, capture, "platform-2", "face-owner-beta-reinserted", [
    face.internals.form?.id,
    alpha.elements.length,
    beta.elements.length,
    face.history.join(","),
  ]);

  return [
    fact("initial", first),
    fact("final", second),
    fact("callbacks", face.history.join("|")),
    fact("alpha-elements", alpha.elements.length),
    fact("beta-elements", beta.elements.length),
  ];
}

async function disabledFieldsetLegendTransition(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const name = elementName(spec, "disabled-face");

  class DisabledFace extends HTMLElement implements FormFace {
    static formAssociated = true;
    readonly internals = this.attachInternals();
    readonly disabledHistory: boolean[] = [];

    formDisabledCallback(disabled: boolean): void {
      this.disabledHistory.push(disabled);
      this.dataset.disabledCallback = String(disabled);
    }
  }
  customElements.define(name, DisabledFace);

  const form = document.createElement("form");
  const fieldset = document.createElement("fieldset");
  const legend = document.createElement("legend");
  legend.textContent = "Exempt";
  const legendFace = document.createElement(name) as DisabledFace;
  legendFace.setAttribute("name", "legend-face");
  legendFace.internals.setFormValue(`legend-${spec.seed}`);
  legend.append(legendFace);
  const inside = document.createElement(name) as DisabledFace;
  inside.setAttribute("name", "inside-face");
  inside.internals.setFormValue(`inside-${spec.variant}`);
  fieldset.append(legend, inside);
  const outside = document.createElement(name) as DisabledFace;
  outside.setAttribute("name", "outside-face");
  outside.internals.setFormValue("outside");
  form.append(fieldset, outside);
  root.append(form);

  fieldset.disabled = true;
  const first = formDataLabel(form);
  assertFixture(inside.matches(":disabled"), "FACE inside disabled fieldset became disabled");
  assertFixture(legendFace.matches(":enabled"), "FACE in first legend stayed enabled");
  assertFixture(new FormData(form).get("inside-face") === null, "disabled FACE was omitted from FormData");
  output(
    root,
    "disabled-first",
    `${first}\ninside=${inside.disabledHistory.join(",")}\nlegend=${legendFace.disabledHistory.join(",")}`,
  );
  await capturePlatformStep(host, capture, "platform-1", "face-fieldset-disabled", [
    inside.matches(":disabled"),
    legendFace.matches(":enabled"),
    first,
    inside.disabledHistory.join(","),
  ]);

  legend.append(inside);
  outside.toggleAttribute("disabled", true);
  outside.toggleAttribute("disabled", false);
  await microtaskTurns(2);
  const second = formDataLabel(form);
  assertFixture(inside.matches(":enabled"), "moving FACE into first legend enabled it");
  assertFixture(new FormData(form).get("inside-face") === `inside-${spec.variant}`, "enabled FACE rejoined FormData");
  output(
    root,
    "disabled-second",
    `${second}\ninside=${inside.disabledHistory.join(",")}\noutside=${outside.disabledHistory.join(",")}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "face-legend-reparented", [
    inside.matches(":enabled"),
    second,
    inside.disabledHistory.join(","),
    outside.disabledHistory.join(","),
  ]);

  return [
    fact("disabled-form-data", first),
    fact("enabled-form-data", second),
    fact("inside-callbacks", inside.disabledHistory.join("|")),
    fact("outside-callbacks", outside.disabledHistory.join("|")),
    fact("legend-callbacks", legendFace.disabledHistory.join("|")),
  ];
}

async function internalsValiditySubmitTransition(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const name = elementName(spec, "validity-face");

  class ValidityFace extends HTMLElement implements FormFace {
    static formAssociated = true;
    readonly internals = this.attachInternals();
    readonly anchor: HTMLInputElement;

    constructor() {
      super();
      const shadow = this.attachShadow({ mode: "open" });
      this.anchor = document.createElement("input");
      this.anchor.setAttribute("aria-label", "validation anchor");
      shadow.append(this.anchor);
      this.internals.setFormValue(`validity-${spec.seed}`);
    }
  }
  customElements.define(name, ValidityFace);

  const form = document.createElement("form");
  const face = document.createElement(name) as ValidityFace;
  face.setAttribute("name", "validated");
  const submitter = document.createElement("button");
  submitter.type = "submit";
  submitter.name = "action";
  submitter.value = `save-${spec.variant}`;
  form.append(face, submitter);
  root.append(form);

  const invalidEvents: string[] = [];
  face.addEventListener("invalid", (event) => {
    invalidEvents.push(`face:${event.cancelable}:${event.target === face}`);
  });
  form.addEventListener("invalid", () => invalidEvents.push("form-capture"), true);
  face.internals.setValidity({ customError: true }, `blocked-${spec.seed}`, face.anchor);
  const formValid = form.checkValidity();
  const reported = face.internals.reportValidity();
  const first = [
    face.internals.validity.valid,
    face.internals.validity.customError,
    face.internals.validationMessage,
    face.matches(":invalid"),
    form.matches(":invalid"),
    invalidEvents.join("|"),
  ].join(";");
  assertFixture(!formValid && !reported, "invalid FACE blocked aggregate form validity");
  output(root, "validity-invalid", first);
  await capturePlatformStep(host, capture, "platform-1", "face-validity-invalid", [
    formValid,
    reported,
    face.internals.validationMessage,
    invalidEvents.length,
  ]);

  const submits: string[] = [];
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    submits.push(`${(event as SubmitEvent).submitter === submitter}:${formDataLabel(form)}`);
  });
  face.internals.setValidity({});
  form.requestSubmit(submitter);
  const second = [
    face.internals.validity.valid,
    face.matches(":valid"),
    form.matches(":valid"),
    submits.join("|"),
  ].join(";");
  assertFixture(submits.length === 1, "valid FACE allowed requestSubmit");
  output(root, "validity-valid", second);
  await capturePlatformStep(host, capture, "platform-2", "face-validity-submit", [
    form.checkValidity(),
    face.internals.checkValidity(),
    submits.join("|"),
    face.internals.validationMessage,
  ]);

  return [
    fact("invalid", first),
    fact("valid", second),
    fact("invalid-events", invalidEvents.join("|")),
    fact("submits", submits.join("|")),
    fact("will-validate", face.internals.willValidate),
  ];
}

async function resetCallbackFormState(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const name = elementName(spec, "reset-face");

  class ResetFace extends HTMLElement implements FormFace {
    static formAssociated = true;
    readonly internals = this.attachInternals();
    readonly history: string[] = [];
    defaultValue = "";
    currentValue = "";

    setDefault(value: string): void {
      this.defaultValue = value;
      this.setValue(value);
    }

    setValue(value: string): void {
      this.currentValue = value;
      this.internals.setFormValue(value, value);
      this.textContent = value;
      this.dataset.value = value;
    }

    formResetCallback(): void {
      this.history.push(`reset:${this.currentValue}->${this.defaultValue}`);
      this.setValue(this.defaultValue);
    }
  }
  customElements.define(name, ResetFace);

  const form = document.createElement("form");
  const face = document.createElement(name) as ResetFace;
  face.setAttribute("name", "custom-reset");
  face.setDefault(`default-${spec.seed}`);
  const input = document.createElement("input");
  input.name = "builtin-reset";
  input.defaultValue = `native-${spec.variant}`;
  form.append(face, input);
  root.append(form);

  face.setValue(`dirty-${spec.variant}`);
  input.value = `dirty-native-${spec.seed}`;
  const first = formDataLabel(form);
  output(root, "reset-dirty", `${first}\n${face.textContent}\n${input.value}`);
  await capturePlatformStep(host, capture, "platform-1", "face-reset-dirty", [
    first,
    face.currentValue,
    input.value,
    face.history.length,
  ]);

  form.reset();
  const second = formDataLabel(form);
  assertFixture(face.currentValue === face.defaultValue, "form reset invoked FACE reset callback");
  assertFixture(input.value === input.defaultValue, "form reset restored native control value");
  output(root, "reset-default", `${second}\n${face.history.join("|")}\n${input.value}`);
  await capturePlatformStep(host, capture, "platform-2", "face-reset-default", [
    second,
    face.currentValue,
    input.value,
    face.history.join("|"),
  ]);

  return [
    fact("dirty", first),
    fact("reset", second),
    fact("callbacks", face.history.join("|")),
    fact("custom-value", face.currentValue),
    fact("native-value", input.value),
  ];
}

async function labelAssociationRetarget(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const name = elementName(spec, "label-face");

  class LabelFace extends HTMLElement implements FormFace {
    static formAssociated = true;
    readonly internals = this.attachInternals();
    clicks = 0;

    constructor() {
      super();
      this.addEventListener("click", () => {
        this.clicks += 1;
        this.dataset.clicks = String(this.clicks);
        this.focus();
      });
    }
  }
  customElements.define(name, LabelFace);

  const form = document.createElement("form");
  const label = document.createElement("label");
  label.textContent = "Choose custom control";
  const firstFace = document.createElement(name) as LabelFace;
  firstFace.id = `label-first-${spec.seed}`;
  firstFace.tabIndex = 0;
  const secondFace = document.createElement(name) as LabelFace;
  secondFace.id = `label-second-${spec.seed}`;
  secondFace.tabIndex = 0;
  label.htmlFor = firstFace.id;
  form.append(label, firstFace, secondFace);
  root.append(form);

  label.click();
  const first = [
    label.control === firstFace,
    firstFace.internals.labels.length,
    secondFace.internals.labels.length,
    firstFace.clicks,
    document.activeElement === firstFace,
  ].join("|");
  assertFixture(firstFace.clicks === 1, "label activation clicked its first FACE control");
  output(root, "label-first", first);
  await capturePlatformStep(host, capture, "platform-1", "face-label-first", [
    first,
    firstFace.internals.labels[0] === label,
    label.form === form,
  ]);

  label.htmlFor = secondFace.id;
  label.click();
  const second = [
    label.control === secondFace,
    firstFace.internals.labels.length,
    secondFace.internals.labels.length,
    firstFace.clicks,
    secondFace.clicks,
    document.activeElement === secondFace,
  ].join("|");
  assertFixture(secondFace.clicks === 1, "retargeted label activated the second FACE control");
  output(root, "label-second", second);
  await capturePlatformStep(host, capture, "platform-2", "face-label-second", [
    second,
    secondFace.internals.labels[0] === label,
    label.form === form,
  ]);

  return [
    fact("initial", first),
    fact("retargeted", second),
    fact("first-clicks", firstFace.clicks),
    fact("second-clicks", secondFace.clicks),
    fact("label-control", (label.control as Element | null)?.id ?? "null"),
  ];
}

async function delayedUpgradeAssociation(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const name = elementName(spec, "upgrade-face");
  const form = document.createElement("form");
  form.id = `upgrade-form-${spec.seed}`;
  form.innerHTML = `<input name="native" value="before"><${name} id="inside" name="inside"></${name}>`;
  const external = document.createElement(name);
  external.id = "external";
  external.setAttribute("name", "external");
  external.setAttribute("form", form.id);
  root.append(form, external);

  const beforeInside = form.querySelector(name) as HTMLElement;
  const first = [
    beforeInside.constructor.name,
    external.constructor.name,
    form.elements.length,
    formDataLabel(form),
    beforeInside.matches(":defined"),
  ].join("|");
  assertFixture(!beforeInside.matches(":defined"), "candidate stayed undefined before registry definition");
  output(root, "upgrade-before", first);
  await capturePlatformStep(host, capture, "platform-1", "face-upgrade-candidates", [first]);

  class UpgradeFace extends HTMLElement implements FormFace {
    static formAssociated = true;
    readonly internals = this.attachInternals();
    readonly history: string[] = [];

    constructor() {
      super();
      this.internals.setFormValue(`upgraded-${this.id}-${spec.variant}`);
      this.dataset.constructed = "true";
    }

    connectedCallback(): void {
      this.history.push("connected");
    }

    formAssociatedCallback(owner: HTMLFormElement | null): void {
      this.history.push(`form:${owner?.id ?? "null"}`);
    }
  }
  customElements.define(name, UpgradeFace);
  await customElements.whenDefined(name);
  await microtaskTurns(2);
  const inside = beforeInside as UpgradeFace;
  const upgradedExternal = external as UpgradeFace;
  const second = [
    inside instanceof UpgradeFace,
    upgradedExternal instanceof UpgradeFace,
    form.elements.length,
    formDataLabel(form),
    inside.history.join(","),
    upgradedExternal.history.join(","),
  ].join("|");
  assertFixture(inside instanceof UpgradeFace && upgradedExternal instanceof UpgradeFace, "definition upgraded all candidates");
  assertFixture(form.elements.length === 3, "upgraded FACE controls joined form.elements");
  output(root, "upgrade-after", second);
  await capturePlatformStep(host, capture, "platform-2", "face-upgrade-associated", [
    second,
    inside.matches(":defined"),
    upgradedExternal.internals.form === form,
  ]);

  return [
    fact("before", first),
    fact("after", second),
    fact("form-data", formDataLabel(form)),
    fact("inside-history", inside.history.join("|")),
    fact("external-history", upgradedExternal.history.join("|")),
  ];
}

async function adoptReconnectLifecycle(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const name = elementName(spec, "adopt-face");

  class AdoptFace extends HTMLElement implements FormFace {
    static formAssociated = true;
    readonly internals = this.attachInternals();
    readonly history: string[] = [];

    constructor() {
      super();
      this.internals.setFormValue(`adopt-${spec.seed}`);
    }

    connectedCallback(): void {
      this.history.push(`connected:${this.ownerDocument === document ? "main" : "other"}`);
    }

    disconnectedCallback(): void {
      this.history.push(`disconnected:${this.ownerDocument === document ? "main" : "other"}`);
    }

    adoptedCallback(oldDocument: Document, newDocument: Document): void {
      this.history.push(`adopted:${oldDocument === document ? "main" : "other"}->${newDocument === document ? "main" : "other"}`);
    }

    formAssociatedCallback(owner: HTMLFormElement | null): void {
      this.history.push(`form:${owner?.id ?? "null"}`);
    }
  }
  customElements.define(name, AdoptFace);

  const firstForm = document.createElement("form");
  firstForm.id = `adopt-first-${spec.seed}`;
  const face = document.createElement(name) as AdoptFace;
  face.setAttribute("name", "adopted");
  firstForm.append(face);
  root.append(firstForm);
  const first = `${face.internals.form?.id}|${formDataLabel(firstForm)}|${face.history.join(",")}`;
  output(root, "adopt-first", first);
  await capturePlatformStep(host, capture, "platform-1", "face-adopt-main", [
    face.internals.form?.id,
    formDataLabel(firstForm),
    face.history.join(","),
  ]);

  const detached = document.implementation.createHTMLDocument("detached owner");
  detached.adoptNode(face);
  detached.body.append(face);
  const detachedOwner = face.internals.form;
  document.adoptNode(face);
  const secondForm = document.createElement("form");
  secondForm.id = `adopt-second-${spec.seed}`;
  secondForm.append(face);
  root.append(secondForm);
  await microtaskTurns(2);
  const second = `${face.internals.form?.id}|${formDataLabel(secondForm)}|detached=${detachedOwner === null}|${face.history.join(",")}`;
  assertFixture(detachedOwner === null, "FACE lost its form owner in detached document");
  assertFixture(face.internals.form === secondForm, "FACE gained the second form owner after adoption back");
  output(root, "adopt-second", second);
  await capturePlatformStep(host, capture, "platform-2", "face-adopt-returned", [
    face.internals.form?.id,
    formDataLabel(secondForm),
    face.history.join(","),
  ]);

  return [
    fact("initial", first),
    fact("final", second),
    fact("detached-owner-null", detachedOwner === null),
    fact("history", face.history.join("|")),
    fact("owner-document-main", face.ownerDocument === document),
  ];
}

async function customStateShadowUpgrade(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const name = elementName(spec, "state-shadow");
  const style = document.createElement("style");
  style.textContent = `
    ${name}:state(--active) { color: rgb(0, 128, 0); }
    ${name}:state(--pending) { color: rgb(255, 165, 0); }
  `;
  const lightCandidate = document.createElement(name);
  lightCandidate.id = `state-light-${spec.seed}`;
  const shadowHost = document.createElement("div");
  shadowHost.dataset.stateShadowHost = "";
  const shadow = shadowHost.attachShadow({ mode: "open" });
  shadow.innerHTML = `<${name} id="state-shadow-${spec.seed}"></${name}>`;
  const shadowCandidate = shadow.querySelector(name);
  assertFixture(shadowCandidate, "shadow custom-state candidate exists");
  root.append(style, lightCandidate, shadowHost);

  const first = [
    lightCandidate.matches(":defined"),
    shadowCandidate.matches(":defined"),
    lightCandidate.constructor.name,
    shadowCandidate.constructor.name,
  ].join("|");
  output(root, "state-before-upgrade", first);
  await capturePlatformStep(host, capture, "platform-1", "custom-state-candidates", [first]);

  class StatefulElement extends HTMLElement implements FormFace {
    readonly internals = this.attachInternals();

    connectedCallback(): void {
      this.dataset.upgraded = "true";
      this.textContent = `stateful-${this.id}`;
    }

    setState(remove: string | null, add: string): void {
      if (remove) this.internals.states.delete(remove);
      this.internals.states.add(add);
      this.dataset.state = add;
    }
  }
  customElements.define(name, StatefulElement);
  await microtaskTurns(2);
  const light = lightCandidate as StatefulElement;
  const inShadow = shadowCandidate as StatefulElement;
  light.setState(null, "--active");
  inShadow.setState(null, "--pending");
  const activeSnapshot = [
    light instanceof StatefulElement,
    inShadow instanceof StatefulElement,
    light.matches(":state(--active)"),
    inShadow.matches(":state(--pending)"),
    getComputedStyle(light).color,
    getComputedStyle(inShadow).color,
  ].join("|");
  light.setState("--active", "--pending");
  inShadow.setState("--pending", "--active");
  const second = [
    activeSnapshot,
    light.matches(":state(--pending)"),
    inShadow.matches(":state(--active)"),
    getComputedStyle(light).color,
    getComputedStyle(inShadow).color,
  ].join("|");
  assertFixture(light.matches(":state(--pending)"), "light custom state transitioned to pending");
  assertFixture(inShadow.matches(":state(--active)"), "shadow custom state transitioned to active");
  output(root, "state-after-upgrade", second);
  await capturePlatformStep(host, capture, "platform-2", "custom-state-shadow-upgraded", [second]);

  return [
    fact("initial", first),
    fact("active-snapshot", activeSnapshot),
    fact("final", second),
    fact("light-state", light.dataset.state ?? "missing"),
    fact("shadow-state", inShadow.dataset.state ?? "missing"),
  ];
}

const SCENARIOS: Record<string, CustomElementScenario> = {
  "form-value-string-transition": formValueStringTransition,
  "formdata-multi-value-transition": formDataMultiValueTransition,
  "dynamic-form-owner-reassociation": dynamicFormOwnerReassociation,
  "disabled-fieldset-legend-transition": disabledFieldsetLegendTransition,
  "internals-validity-submit-transition": internalsValiditySubmitTransition,
  "reset-callback-form-state": resetCallbackFormState,
  "label-association-retarget": labelAssociationRetarget,
  "delayed-upgrade-association": delayedUpgradeAssociation,
  "adopt-reconnect-lifecycle": adoptReconnectLifecycle,
  "custom-state-shadow-upgrade": customStateShadowUpgrade,
};

export async function runCustomElementsFormBoundaryCase(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformBoundaryResult> {
  const scenario = SCENARIOS[spec.slug];
  assertFixture(scenario, `missing custom-elements/form scenario ${spec.slug}`);
  const facts = await scenario(host, meta, spec, capture);
  return { status: "ready", facts };
}

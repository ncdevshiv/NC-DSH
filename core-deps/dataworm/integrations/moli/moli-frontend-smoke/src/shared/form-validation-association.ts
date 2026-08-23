import { assertFixture } from "./harness";
import {
  capturePlatformStep,
  fact,
  type CapturePlatformFrame,
  type PlatformBoundaryResult,
  type PlatformFact,
} from "./platform-boundary";
import type { CaseSpec, SmokeMeta } from "./types";

type FormScenario = (
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
) => Promise<PlatformFact[]>;

type ValidatedControl = HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement;

function scenarioRoot(host: HTMLElement, name: string): HTMLElement {
  const root = document.createElement("section");
  root.dataset.formScenario = name;
  host.append(root);
  return root;
}

function output(root: HTMLElement, name: string, value: string): HTMLOutputElement {
  const element = document.createElement("output");
  element.dataset.formOutput = name;
  element.textContent = value;
  root.append(element);
  return element;
}

function validityLabel(control: ValidatedControl): string {
  const validity = control.validity;
  return [
    `valid=${validity.valid}`,
    `missing=${validity.valueMissing}`,
    `type=${validity.typeMismatch}`,
    `pattern=${validity.patternMismatch}`,
    `underflow=${validity.rangeUnderflow}`,
    `overflow=${validity.rangeOverflow}`,
    `step=${validity.stepMismatch}`,
    `short=${validity.tooShort}`,
    `long=${validity.tooLong}`,
    `bad=${validity.badInput}`,
    `custom=${validity.customError}`,
  ].join(",");
}

function formDataLabel(data: FormData): string {
  return Array.from(data.entries(), ([name, value]) =>
    `${name}=${typeof value === "string" ? value : `${value.name}:${value.type}:${value.size}`}`,
  ).join("|");
}

function controlIds(collection: HTMLCollectionOf<Element> | NodeListOf<HTMLLabelElement>): string {
  return Array.from(collection, (element) => element.id || element.localName).join("|");
}

async function requiredPatternValidity(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const form = document.createElement("form");
  form.id = `account-${spec.seed}`;
  const label = document.createElement("label");
  label.textContent = "Account email";
  const input = document.createElement("input");
  input.id = `email-${spec.seed}`;
  input.name = "email";
  input.type = "email";
  input.required = true;
  input.pattern = "[^@]+@example\\.test";
  label.htmlFor = input.id;
  form.append(label, input);
  root.append(form);

  const invalidTargets: string[] = [];
  form.addEventListener(
    "invalid",
    (event) => invalidTargets.push((event.target as Element).id || "anonymous"),
    true,
  );

  const empty = validityLabel(input);
  const emptyFormValid = form.checkValidity();
  input.value = `person-${spec.variant}@else.test`;
  const wrongDomain = validityLabel(input);
  const wrongFormValid = form.checkValidity();
  assertFixture(input.validity.patternMismatch, "wrong email domain mismatched the pattern");
  assertFixture(!wrongFormValid, "pattern mismatch invalidated the form");
  output(root, "required-pattern-first", `${empty}\n${wrongDomain}\n${invalidTargets.join("|")}`);
  await capturePlatformStep(host, capture, "platform-1", "required-pattern-invalid", [
    emptyFormValid,
    wrongFormValid,
    invalidTargets.length,
    input.validationMessage.length > 0,
  ]);

  input.value = `person-${spec.seed}@example.test`;
  const valid = validityLabel(input);
  const validForm = form.checkValidity();
  assertFixture(validForm && input.validity.valid, "matching email satisfied all constraints");
  output(root, "required-pattern-second", `${valid}\n${input.value}`);
  await capturePlatformStep(host, capture, "platform-2", "required-pattern-valid", [
    validForm,
    input.value,
    input.labels?.length ?? 0,
    invalidTargets.length,
  ]);

  return [
    fact("empty", empty),
    fact("wrong-domain", wrongDomain),
    fact("valid", valid),
    fact("invalid-targets", invalidTargets.join("|")),
    fact("form-elements", form.elements.length),
  ];
}

async function numberRangeStepValidity(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const form = document.createElement("form");
  const input = document.createElement("input");
  input.type = "number";
  input.name = "quantity";
  input.required = true;
  input.min = "2";
  input.max = "12";
  input.step = "2";
  form.append(input);
  root.append(form);

  input.value = "3";
  const misaligned = validityLabel(input);
  assertFixture(input.validity.stepMismatch, "misaligned number exposed stepMismatch");
  output(root, "number-misaligned", `${input.value}:${input.valueAsNumber}:${misaligned}`);
  await capturePlatformStep(host, capture, "platform-1", "number-step-mismatch", [
    input.checkValidity(),
    input.valueAsNumber,
    input.validity.stepMismatch,
    form.checkValidity(),
  ]);

  input.value = "13";
  const overflow = validityLabel(input);
  assertFixture(input.validity.rangeOverflow, "number above max exposed rangeOverflow");
  input.value = "8";
  input.stepUp(2);
  input.stepDown();
  const stepped = validityLabel(input);
  assertFixture(input.value === "10", "stepUp and stepDown used the min-derived step grid");
  assertFixture(input.validity.valid, "stepped value remained valid");
  output(root, "number-stepped", `${overflow}\n${input.value}:${stepped}`);
  await capturePlatformStep(host, capture, "platform-2", "number-step-valid", [
    input.value,
    input.valueAsNumber,
    input.checkValidity(),
    form.checkValidity(),
  ]);

  return [
    fact("misaligned", misaligned),
    fact("overflow", overflow),
    fact("stepped", stepped),
    fact("final-value", input.value),
    fact("range", `${input.min}|${input.max}|${input.step}`),
  ];
}

async function customValidityReset(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const form = document.createElement("form");
  const textarea = document.createElement("textarea");
  textarea.name = "notes";
  textarea.defaultValue = `baseline-${spec.seed}`;
  const input = document.createElement("input");
  input.name = "code";
  input.defaultValue = `code-${spec.variant}`;
  form.append(textarea, input);
  root.append(form);
  const resets: string[] = [];
  form.addEventListener("reset", () => resets.push(`${textarea.value}|${input.value}`));

  textarea.value = `dirty-${spec.variant}`;
  input.value = `dirty-code-${spec.seed}`;
  textarea.setCustomValidity(`Custom ${spec.seed}`);
  const custom = validityLabel(textarea);
  assertFixture(textarea.validity.customError, "custom validity marked textarea invalid");
  assertFixture(textarea.validationMessage === `Custom ${spec.seed}`, "custom message was retained");
  output(root, "custom-before-reset", `${textarea.value}\n${textarea.validationMessage}\n${custom}`);
  await capturePlatformStep(host, capture, "platform-1", "custom-validity", [
    textarea.checkValidity(),
    form.checkValidity(),
    textarea.value,
    input.value,
  ]);

  form.reset();
  const afterReset = `${textarea.value}|${input.value}|${textarea.validity.customError}`;
  assertFixture(textarea.value === textarea.defaultValue, "reset restored textarea default value");
  assertFixture(input.value === input.defaultValue, "reset restored input default value");
  assertFixture(textarea.validity.customError, "reset did not clear custom validity");
  textarea.setCustomValidity("");
  const cleared = validityLabel(textarea);
  assertFixture(form.checkValidity(), "clearing custom validity restored form validity");
  output(root, "custom-after-reset", `${afterReset}\n${cleared}\n${resets.join("|")}`);
  await capturePlatformStep(host, capture, "platform-2", "custom-reset-cleared", [
    textarea.value,
    input.value,
    textarea.validity.customError,
    resets.length,
  ]);

  return [
    fact("custom", custom),
    fact("after-reset", afterReset),
    fact("cleared", cleared),
    fact("reset-events", resets.join("|")),
    fact("final-valid", form.checkValidity()),
  ];
}

async function disabledFieldsetLegend(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const form = document.createElement("form");
  const fieldset = document.createElement("fieldset");
  fieldset.disabled = true;
  fieldset.name = "scope";
  const oldLegend = document.createElement("legend");
  oldLegend.id = "old-legend";
  oldLegend.textContent = "Primary";
  const oldLegendInput = document.createElement("input");
  oldLegendInput.name = "legend-value";
  oldLegendInput.value = `old-${spec.seed}`;
  oldLegendInput.required = true;
  oldLegend.append(oldLegendInput);
  const blocked = document.createElement("input");
  blocked.name = "blocked-value";
  blocked.value = `blocked-${spec.variant}`;
  blocked.required = true;
  const nested = document.createElement("fieldset");
  const nestedInput = document.createElement("input");
  nestedInput.name = "nested-value";
  nestedInput.value = "nested";
  nested.append(nestedInput);
  fieldset.append(oldLegend, blocked, nested);
  form.append(fieldset);
  root.append(form);

  const firstEntries = formDataLabel(new FormData(form));
  assertFixture(oldLegendInput.willValidate, "first legend descendant escaped disabled fieldset");
  assertFixture(!blocked.willValidate, "ordinary fieldset descendant was barred from validation");
  output(
    root,
    "fieldset-first",
    `${oldLegendInput.willValidate}|${blocked.willValidate}|${nestedInput.willValidate}\n${firstEntries}`,
  );
  await capturePlatformStep(host, capture, "platform-1", "fieldset-first-legend", [
    fieldset.elements.length,
    oldLegendInput.willValidate,
    blocked.willValidate,
    firstEntries,
  ]);

  const newLegend = document.createElement("legend");
  newLegend.id = "new-legend";
  newLegend.textContent = "Replacement";
  const newLegendInput = document.createElement("input");
  newLegendInput.name = "replacement-value";
  newLegendInput.value = `new-${spec.variant}`;
  newLegendInput.required = true;
  newLegend.append(newLegendInput);
  fieldset.prepend(newLegend);
  const secondEntries = formDataLabel(new FormData(form));
  assertFixture(!oldLegendInput.willValidate, "former first legend lost its disabled exemption");
  assertFixture(newLegendInput.willValidate, "new first legend gained the disabled exemption");
  assertFixture(!blocked.willValidate, "blocked control remained barred after legend insertion");
  output(
    root,
    "fieldset-second",
    `${newLegendInput.willValidate}|${oldLegendInput.willValidate}|${blocked.willValidate}\n${secondEntries}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "fieldset-new-first-legend", [
    fieldset.elements.length,
    newLegendInput.willValidate,
    oldLegendInput.willValidate,
    secondEntries,
  ]);

  return [
    fact("first-entries", firstEntries),
    fact("second-entries", secondEntries),
    fact("legend-order", Array.from(fieldset.children, (node) => node.id || node.localName).join("|")),
    fact("fieldset-elements", fieldset.elements.length),
    fact("form-valid", form.checkValidity()),
  ];
}

async function dynamicFormOwner(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const alpha = document.createElement("form");
  alpha.id = `alpha-${spec.seed}`;
  const beta = document.createElement("form");
  beta.id = `beta-${spec.seed}`;
  const external = document.createElement("input");
  external.name = "external";
  external.value = `value-${spec.variant}`;
  external.setAttribute("form", alpha.id);
  root.append(alpha, beta, external);

  const ownerHistory: string[] = [];
  const recordOwner = (label: string): void => {
    ownerHistory.push(`${label}:${external.form?.id ?? "null"}`);
  };
  recordOwner("initial");
  assertFixture(external.form === alpha, "explicit form attribute associated the external input");
  output(root, "owner-initial", `${ownerHistory.join("|")}\n${formDataLabel(new FormData(alpha))}`);
  await capturePlatformStep(host, capture, "platform-1", "form-owner-alpha", [
    external.form?.id ?? "null",
    alpha.elements.length,
    beta.elements.length,
    formDataLabel(new FormData(alpha)),
  ]);

  external.setAttribute("form", beta.id);
  recordOwner("beta");
  assertFixture(external.form === beta, "changing form attribute moved the explicit owner");
  beta.id = `stale-${spec.seed}`;
  recordOwner("missing");
  assertFixture(external.form === null, "removing the referenced id cleared form ownership");
  const replacement = document.createElement("form");
  replacement.id = `beta-${spec.seed}`;
  root.insertBefore(replacement, beta);
  recordOwner("replacement");
  assertFixture(external.form === replacement, "new matching form id restored ownership");
  const replacementEntries = formDataLabel(new FormData(replacement));
  output(root, "owner-replacement", `${ownerHistory.join("|")}\n${replacementEntries}`);
  await capturePlatformStep(host, capture, "platform-2", "form-owner-replacement", [
    (external.form as HTMLFormElement | null)?.id ?? "null",
    alpha.elements.length,
    beta.elements.length,
    replacement.elements.length,
  ]);

  return [
    fact("owners", ownerHistory.join("|")),
    fact("replacement-entries", replacementEntries),
    fact("form-attribute", external.getAttribute("form") ?? "missing"),
    fact("replacement-first", replacement.nextElementSibling === beta),
  ];
}

async function requestSubmitSubmitter(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const form = document.createElement("form");
  form.id = `submit-${spec.seed}`;
  form.action = `/support/form-submit/${spec.seed}`;
  form.method = "post";
  const required = document.createElement("input");
  required.name = "query";
  required.required = true;
  required.value = `ready-${spec.variant}`;
  const primary = document.createElement("button");
  primary.type = "submit";
  primary.id = "primary-submit";
  primary.name = "intent";
  primary.value = "primary";
  const secondary = document.createElement("button");
  secondary.type = "submit";
  secondary.id = "secondary-submit";
  secondary.name = "intent";
  secondary.value = "secondary";
  secondary.formAction = `/support/form-secondary/${spec.seed}`;
  secondary.formMethod = "get";
  form.append(required, primary, secondary);
  root.append(form);

  const submits: string[] = [];
  const invalids: string[] = [];
  required.addEventListener("invalid", () => invalids.push(required.value || "empty"));
  form.addEventListener("submit", (event) => {
    assertFixture(event instanceof SubmitEvent, "requestSubmit dispatched a SubmitEvent");
    event.preventDefault();
    const submitter = event.submitter;
    const data = new FormData(form, submitter ?? undefined);
    submits.push(
      `${submitter?.id ?? "null"}:${submitter instanceof HTMLButtonElement ? submitter.formMethod : "none"}:${formDataLabel(data)}`,
    );
  });

  form.requestSubmit(primary);
  assertFixture(submits.length === 1, "primary requestSubmit synchronously submitted once");
  output(root, "submit-primary", submits.join("\n"));
  await capturePlatformStep(host, capture, "platform-1", "request-submit-primary", [
    submits.length,
    invalids.length,
    new URL(primary.formAction).pathname,
    primary.formMethod,
  ]);

  required.value = "";
  form.requestSubmit(secondary);
  assertFixture(
    submits.length === 1 && invalids.length === 1,
    "invalid request was gated before submit",
  );
  required.value = `restored-${spec.seed}`;
  form.requestSubmit(secondary);
  assertFixture(
    Number(submits.length) === 2,
    "secondary requestSubmit submitted after validity recovery",
  );
  output(root, "submit-secondary", `${submits.join("\n")}\ninvalid:${invalids.join("|")}`);
  await capturePlatformStep(host, capture, "platform-2", "request-submit-secondary", [
    submits.length,
    invalids.length,
    new URL(secondary.formAction).pathname,
    secondary.formMethod,
  ]);

  return [
    fact("submits", submits.join("|")),
    fact("invalids", invalids.join("|")),
    fact("default-action", new URL(form.action).pathname),
    fact("secondary-action", new URL(secondary.formAction).pathname),
    fact("final-valid", form.checkValidity()),
  ];
}

async function formdataSuccessfulControls(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const form = document.createElement("form");
  form.innerHTML = `
    <input data-text name="alpha" value="A-${spec.seed}">
    <input data-disabled name="disabled" value="D-${spec.variant}" disabled>
    <input data-check-x type="checkbox" name="tag" value="x" checked>
    <input data-check-y type="checkbox" name="tag" value="y">
    <input data-radio-fast type="radio" name="mode" value="fast" checked>
    <input data-radio-slow type="radio" name="mode" value="slow">
    <select data-select name="pick" multiple>
      <option value="one" selected>One</option>
      <option value="two">Two</option>
      <option value="three" selected>Three</option>
    </select>
    <textarea name="note">N-${spec.variant}</textarea>
    <input value="unnamed">
    <button type="submit" name="submitter" value="ignored">Submit</button>`;
  root.append(form);
  const text = form.querySelector("[data-text]");
  const disabled = form.querySelector("[data-disabled]");
  const checkY = form.querySelector("[data-check-y]");
  const radioSlow = form.querySelector("[data-radio-slow]");
  const select = form.querySelector("[data-select]");
  assertFixture(text instanceof HTMLInputElement, "text control exists");
  assertFixture(disabled instanceof HTMLInputElement, "disabled control exists");
  assertFixture(checkY instanceof HTMLInputElement, "second checkbox exists");
  assertFixture(radioSlow instanceof HTMLInputElement, "second radio exists");
  assertFixture(select instanceof HTMLSelectElement, "multiple select exists");

  let formdataEvents = 0;
  form.addEventListener("formdata", (event) => {
    assertFixture(event instanceof FormDataEvent, "FormData construction dispatched FormDataEvent");
    formdataEvents += 1;
    event.formData.append("event-token", `event-${formdataEvents}`);
  });
  const firstEntries = formDataLabel(new FormData(form));
  assertFixture(!firstEntries.includes("disabled="), "disabled control was not successful");
  assertFixture(!firstEntries.includes("submitter="), "submit button was omitted without submitter");
  output(root, "formdata-first", firstEntries);
  await capturePlatformStep(host, capture, "platform-1", "formdata-initial", [
    firstEntries,
    formdataEvents,
    form.elements.length,
  ]);

  text.name = "renamed";
  disabled.disabled = false;
  checkY.checked = true;
  radioSlow.checked = true;
  select.options[0].selected = false;
  select.options[1].selected = true;
  const secondEntries = formDataLabel(new FormData(form));
  assertFixture(secondEntries.includes("disabled="), "enabled control entered FormData");
  assertFixture(secondEntries.includes("mode=slow"), "new checked radio replaced the old value");
  output(root, "formdata-second", secondEntries);
  await capturePlatformStep(host, capture, "platform-2", "formdata-updated", [
    secondEntries,
    formdataEvents,
    select.selectedOptions.length,
  ]);

  return [
    fact("first", firstEntries),
    fact("second", secondEntries),
    fact("events", formdataEvents),
    fact("selected", Array.from(select.selectedOptions, (option) => option.value).join("|")),
  ];
}

async function radioGroupValidity(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const form = document.createElement("form");
  form.id = `radio-form-${spec.seed}`;
  const inside = document.createElement("input");
  inside.type = "radio";
  inside.name = "plan";
  inside.value = "inside";
  inside.required = true;
  inside.id = "inside-radio";
  const external = document.createElement("input");
  external.type = "radio";
  external.name = "plan";
  external.value = `external-${spec.variant}`;
  external.id = "external-radio";
  external.setAttribute("form", form.id);
  form.append(inside);
  root.append(form, external);
  const events: string[] = [];
  for (const control of [inside, external]) {
    control.addEventListener("input", () => events.push(`input:${control.id}`));
    control.addEventListener("change", () => events.push(`change:${control.id}`));
  }

  const initiallyMissing = validityLabel(inside);
  assertFixture(inside.validity.valueMissing, "unchecked required radio group was missing a value");
  external.click();
  assertFixture(external.checked && !inside.checked, "external radio click updated the group");
  assertFixture(form.checkValidity(), "checked external member satisfied required group");
  const groupedEntries = formDataLabel(new FormData(form));
  output(root, "radio-grouped", `${groupedEntries}\n${events.join("|")}`);
  await capturePlatformStep(host, capture, "platform-1", "radio-group-valid", [
    inside.checked,
    external.checked,
    form.checkValidity(),
    groupedEntries,
  ]);

  external.name = "alternate";
  const separated = validityLabel(inside);
  const separatedEntries = formDataLabel(new FormData(form));
  assertFixture(inside.validity.valueMissing, "renaming checked peer invalidated required group");
  assertFixture(!form.checkValidity(), "separated required group invalidated form");
  output(root, "radio-separated", `${separated}\n${separatedEntries}\n${events.join("|")}`);
  await capturePlatformStep(host, capture, "platform-2", "radio-group-separated", [
    inside.validity.valueMissing,
    external.checked,
    form.checkValidity(),
    separatedEntries,
  ]);

  return [
    fact("initial", initiallyMissing),
    fact("separated", separated),
    fact("events", events.join("|")),
    fact("grouped-entries", groupedEntries),
    fact("separated-entries", separatedEntries),
  ];
}

async function selectLabelAssociation(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const form = document.createElement("form");
  const explicit = document.createElement("label");
  explicit.id = "explicit-label";
  explicit.textContent = "Explicit";
  const wrapping = document.createElement("label");
  wrapping.id = "wrapping-label";
  wrapping.append(document.createTextNode("Wrapping "));
  const select = document.createElement("select");
  select.id = `choice-${spec.seed}`;
  select.name = "choice";
  select.required = true;
  select.innerHTML = `<option value="">Choose</option><option value="alpha">Alpha</option>
    <optgroup label="More"><option value="beta-${spec.variant}">Beta</option></optgroup>`;
  explicit.htmlFor = select.id;
  wrapping.append(select);
  const text = document.createElement("input");
  text.id = `text-${spec.seed}`;
  text.name = "text";
  form.append(explicit, wrapping, text);
  root.append(form);

  const initialLabels = select.labels ? controlIds(select.labels) : "missing";
  const initialValidity = validityLabel(select);
  assertFixture(initialLabels === "explicit-label|wrapping-label", "select exposed explicit and wrapping labels");
  assertFixture(select.validity.valueMissing, "required placeholder option was invalid");
  output(root, "select-initial", `${initialLabels}\n${initialValidity}`);
  await capturePlatformStep(host, capture, "platform-1", "select-label-initial", [
    select.selectedIndex,
    select.value,
    initialLabels,
    form.checkValidity(),
  ]);

  explicit.htmlFor = text.id;
  select.value = "alpha";
  const finalSelectLabels = select.labels ? controlIds(select.labels) : "missing";
  const textLabels = text.labels ? controlIds(text.labels) : "missing";
  assertFixture(finalSelectLabels === "wrapping-label", "retargeted explicit label left wrapping label");
  assertFixture(textLabels === "explicit-label", "explicit label moved to the text input");
  assertFixture(select.validity.valid, "non-placeholder option satisfied required select");
  output(root, "select-final", `${finalSelectLabels}\n${textLabels}\n${validityLabel(select)}`);
  await capturePlatformStep(host, capture, "platform-2", "select-label-retarget", [
    select.selectedIndex,
    select.value,
    finalSelectLabels,
    textLabels,
  ]);

  return [
    fact("initial-labels", initialLabels),
    fact("final-select-labels", finalSelectLabels),
    fact("text-labels", textLabels),
    fact("selected", `${select.selectedIndex}|${select.value}`),
    fact("named-item", form.elements.namedItem("choice") === select),
  ];
}

async function datalistAssociation(
  host: HTMLElement,
  _meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformFact[]> {
  const root = scenarioRoot(host, spec.slug);
  const form = document.createElement("form");
  const label = document.createElement("label");
  label.id = "datalist-label";
  const input = document.createElement("input");
  input.id = `city-${spec.seed}`;
  input.name = "city";
  input.setAttribute("list", `cities-${spec.seed}`);
  label.htmlFor = input.id;
  label.textContent = "City";
  const datalist = document.createElement("datalist");
  datalist.id = `cities-${spec.seed}`;
  datalist.innerHTML = `<option value="Alpha"></option><option value="Beta-${spec.variant}"></option>`;
  form.append(label, input, datalist);
  root.append(form);

  const liveOptions = datalist.options;
  const extra = document.createElement("option");
  extra.value = `Gamma-${spec.seed}`;
  datalist.append(extra);
  assertFixture(input.list === datalist, "list id associated input with datalist");
  assertFixture(liveOptions.length === 3, "datalist options collection stayed live");
  output(
    root,
    "datalist-initial",
    `${input.list?.id ?? "null"}\n${Array.from(liveOptions, (option) => option.value).join("|")}`,
  );
  await capturePlatformStep(host, capture, "platform-1", "datalist-associated", [
    input.list?.id ?? "null",
    liveOptions.length,
    input.labels?.length ?? 0,
    form.elements.length,
  ]);

  datalist.id = `archived-${spec.seed}`;
  const missingOwner = input.list;
  assertFixture(missingOwner === null, "renaming datalist cleared list association");
  const replacement = document.createElement("datalist");
  replacement.id = `cities-${spec.seed}`;
  replacement.innerHTML = `<option value="Delta-${spec.variant}"></option><option value="Epsilon"></option>`;
  form.insertBefore(replacement, datalist);
  input.value = `Delta-${spec.variant}`;
  assertFixture(input.list === replacement, "new matching id restored datalist association");
  output(
    root,
    "datalist-replacement",
    `missing=${missingOwner === null}\n${input.list?.id ?? "null"}\n${Array.from(replacement.options, (option) => option.value).join("|")}`,
  );
  await capturePlatformStep(host, capture, "platform-2", "datalist-reassociated", [
    missingOwner === null,
    input.list?.id ?? "null",
    replacement.options.length,
    input.value,
  ]);

  return [
    fact("original-live-options", liveOptions.length),
    fact("replacement-options", replacement.options.length),
    fact("association", input.list === replacement),
    fact("labels", input.labels ? controlIds(input.labels) : "missing"),
    fact("value", input.value),
  ];
}

const SCENARIOS: Record<string, FormScenario> = {
  "required-pattern-validity": requiredPatternValidity,
  "number-range-step-validity": numberRangeStepValidity,
  "custom-validity-reset": customValidityReset,
  "disabled-fieldset-legend": disabledFieldsetLegend,
  "dynamic-form-owner": dynamicFormOwner,
  "request-submit-submitter": requestSubmitSubmitter,
  "formdata-successful-controls": formdataSuccessfulControls,
  "radio-group-validity": radioGroupValidity,
  "select-label-association": selectLabelAssociation,
  "datalist-association": datalistAssociation,
};

export async function runFormValidationAssociationCase(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<PlatformBoundaryResult> {
  const scenario = SCENARIOS[spec.slug];
  assertFixture(scenario, `missing form validation scenario ${spec.slug}`);
  const facts = await scenario(host, meta, spec, capture);
  return { status: "ready", facts };
}

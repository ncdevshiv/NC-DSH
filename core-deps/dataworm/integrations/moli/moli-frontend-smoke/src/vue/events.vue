<script setup lang="ts">
import { nextTick, onMounted, ref } from "vue";

import { assertFixture, captureFrame, markReady } from "../shared/harness";
import type { CaseSpec, SmokeMeta } from "../shared/types";

const props = defineProps<{ meta: SmokeMeta; spec: CaseSpec }>();
const host = ref<HTMLElement>();
const log = ref<string[]>([]);
function append(value: string) { log.value.push(value); }
function input(event: Event) { append(`input:${(event.currentTarget as HTMLInputElement).value}`); }
function change(event: Event) { append(`change:${(event.currentTarget as HTMLSelectElement).value}`); }
function submit(event: Event) { event.preventDefault(); append("submit:prevented"); }
function keydown(event: KeyboardEvent) { append(`key:${event.key}:${event.ctrlKey}`); }
onMounted(async () => {
  await nextTick();
  await captureFrame(props.meta, "mounted");
  const trigger = host.value?.querySelector<HTMLElement>("[data-trigger]");
  assertFixture(trigger, "event trigger mounted");
  if (props.spec.variant === 3) {
    (trigger as HTMLInputElement).value = `typed-${props.spec.seed}`;
    trigger.dispatchEvent(new InputEvent("input", { bubbles: true, data: "x" }));
  } else if (props.spec.variant === 4) {
    (trigger as HTMLSelectElement).value = "done";
    trigger.dispatchEvent(new Event("change", { bubbles: true }));
  } else if (props.spec.variant === 5) {
    trigger.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  } else if (props.spec.variant === 6 || props.spec.variant === 9) {
    trigger.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: props.spec.variant === 9 ? "k" : "Enter", ctrlKey: props.spec.variant === 9 }));
  } else trigger.click();
  await nextTick();
  assertFixture(log.value.length > 0, "Vue event produced a log entry");
  markReady(props.meta, ["mounted", "vue-native-event"]);
});
</script>

<template>
  <main ref="host" id="smoke-root" data-framework="vue" :data-family="meta.family">
    <h1>{{ meta.title }}</h1>
    <section data-case-body>
      <input v-if="spec.variant === 3" data-trigger value="initial" @input="input">
      <select v-else-if="spec.variant === 4" data-trigger value="new" @change="change"><option value="new">New</option><option value="done">Done</option></select>
      <form v-else-if="spec.variant === 5" data-trigger @submit="submit"><button>Submit</button></form>
      <input v-else-if="spec.variant === 6 || spec.variant === 9" data-trigger aria-label="Command" @keydown="keydown">
      <div v-else-if="spec.variant === 1" @click="append('parent')"><button data-trigger @click="append('child')">Bubble</button></div>
      <div v-else-if="spec.variant === 2" @click="append('parent')"><button data-trigger @click.stop="append('child-stopped')">Stop</button></div>
      <button v-else-if="spec.variant === 7" data-trigger @click="append(`output:${spec.seed}`)">Emit output</button>
      <button v-else-if="spec.variant === 8" data-trigger @click="append('first'); append('second'); append('third')">Handlers</button>
      <button v-else data-trigger @click="append('click:updated')">Click</button>
      <output><ol><li v-for="(entry, index) in log" :key="`${entry}-${index}`">{{ entry }}</li></ol></output>
    </section>
  </main>
</template>

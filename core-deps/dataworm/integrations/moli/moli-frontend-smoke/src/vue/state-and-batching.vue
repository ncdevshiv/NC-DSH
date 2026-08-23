<script setup lang="ts">
import { computed, nextTick, onMounted, ref } from "vue";

import { money, stableItems } from "../shared/data";
import { captureFrame, markReady } from "../shared/harness";
import type { CaseSpec, SmokeMeta } from "../shared/types";

const props = defineProps<{ meta: SmokeMeta; spec: CaseSpec }>();
const count = ref(0);
const enabled = ref(false);
const reduced = ref(props.spec.seed);
const items = stableItems(props.spec.seed, Math.max(4, props.spec.size));
const total = computed(() => items.reduce((sum, item) => sum + item.amount, 0) + count.value);
const summaries = computed(() => [["count", String(count.value)], ["enabled", String(enabled.value)], ["reduced", String(reduced.value)], ["total", money(total.value)]]);
onMounted(async () => {
  await captureFrame(props.meta, "mounted");
  count.value += 1;
  count.value += 2;
  count.value += props.spec.variant + 3;
  enabled.value = true;
  reduced.value += 5 + props.spec.variant;
  await nextTick();
  markReady(props.meta, ["mounted", "vue-batched-state-commit"]);
});
</script>

<template>
  <main id="smoke-root" data-framework="vue" :data-family="meta.family" :data-count="count">
    <h1>{{ meta.title }}</h1>
    <section data-case-body>
      <output v-if="spec.variant === 0" aria-label="count">{{ count }}</output>
      <p v-else-if="spec.variant === 1" :data-batch-result="count">Three queued increments: {{ count }}</p>
      <p v-else-if="spec.variant === 2">Derived total <strong>{{ money(total) }}</strong></p>
      <div v-else-if="spec.variant === 3" role="switch" :aria-checked="enabled" :data-state="enabled ? 'on' : 'off'">Feature {{ enabled ? "enabled" : "disabled" }}</div>
      <p v-else-if="spec.variant === 4">Reducer value <b>{{ reduced }}</b></p>
      <dl v-else-if="spec.variant === 5"><dt>Gross</dt><dd>{{ money(total) }}</dd><dt>Net</dt><dd>{{ money(total - reduced) }}</dd></dl>
      <ol v-else-if="spec.variant === 6"><li v-for="[name, value] in summaries" :key="name" :data-key="name">{{ name }}: {{ value }}</li></ol>
      <section v-else-if="spec.variant === 7"><h2>Computed summary</h2><p v-for="item in items.slice(0, 8)" :key="item.id">{{ item.title }}: {{ money(item.amount + count) }}</p></section>
      <div v-else-if="spec.variant === 8" :data-machine-state="enabled ? 'settled' : 'booting'"><h2>{{ enabled ? "Settled" : "Booting" }}</h2><p>Transition #{{ reduced }}</p></div>
      <div v-else class="metrics"><article v-for="index in 16" :key="index"><h2>Metric {{ index }}</h2><strong>{{ total + (index - 1) * reduced }}</strong><small>{{ enabled ? "live" : "idle" }}</small></article></div>
    </section>
  </main>
</template>

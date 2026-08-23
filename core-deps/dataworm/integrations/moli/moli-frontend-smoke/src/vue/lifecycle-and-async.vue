<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from "vue";

import { stableItems } from "../shared/data";
import { captureFrame, markReady } from "../shared/harness";
import type { CaseSpec, SmokeMeta } from "../shared/types";

const props = defineProps<{ meta: SmokeMeta; spec: CaseSpec }>();
const phase = ref("mounted");
const showChild = ref(true);
const timeline = ref(["render"]);
const items = stableItems(props.spec.seed, props.spec.variant >= 8 ? 24 : 5);
async function commit(label: string) {
  phase.value = label;
  timeline.value.push(label);
  if (props.spec.variant === 7) showChild.value = false;
  await nextTick();
}
function nextAnimationFrame(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => resolve()));
}
onMounted(async () => {
  await captureFrame(props.meta, "mounted");
  if (props.spec.variant === 9) {
    for (let index = 1; index <= 3; index += 1) {
      await nextAnimationFrame();
      await commit(`frame-${index}`);
      await captureFrame(props.meta, `animation-frame-${index}`);
    }
    markReady(props.meta, ["vue-animation-settled"]);
    return;
  }
  const label = props.spec.variant === 4
    ? "timer"
    : props.spec.variant === 2
      ? "microtask"
      : props.spec.variant >= 3
        ? "promise"
        : "watch";
  if (props.spec.variant === 4) {
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
  } else if (props.spec.variant === 2) {
    await new Promise<void>((resolve) => queueMicrotask(resolve));
  } else {
    await Promise.resolve();
  }
  await commit(label);
  markReady(props.meta, [`vue-${label}-settled`]);
});
onBeforeUnmount(() => { document.documentElement.dataset.vueCleanup = "complete"; });
</script>

<template>
  <main id="smoke-root" data-framework="vue" :data-family="meta.family" :data-phase="phase">
    <h1>{{ meta.title }}</h1>
    <section data-case-body>
      <p role="status">Phase: {{ phase }}</p>
      <aside v-if="showChild" id="async-child">Child {{ spec.seed }}</aside>
      <ul v-if="spec.variant >= 5"><li v-for="item in items" :key="item.id">{{ item.title }} — {{ phase }}</li></ul>
      <ol v-if="spec.variant >= 6"><li v-for="(entry, index) in timeline" :key="`${entry}-${index}`">{{ index }}: {{ entry }}</li></ol>
      <div v-if="spec.variant === 9" class="timeline"><article v-for="index in 12" :key="index"><h2>Stage {{ index }}</h2><p>{{ timeline.join(" → ") }}</p></article></div>
    </section>
  </main>
</template>

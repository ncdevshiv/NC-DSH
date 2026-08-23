<script setup lang="ts">
import { nextTick, onMounted, ref } from "vue";

import { deterministicWords } from "../shared/data";
import { captureFrame, markReady } from "../shared/harness";
import type { CaseSpec, SmokeMeta } from "../shared/types";

const props = defineProps<{ meta: SmokeMeta; spec: CaseSpec }>();
const expanded = ref(false);
const sections = Array.from({ length: 12 }, (_, index) => ({
  id: `section-${index}`,
  title: `Section ${index + 1}`,
  text: deterministicWords(props.spec.seed + index, 18),
}));
onMounted(async () => {
  await captureFrame(props.meta, "mounted");
  expanded.value = true;
  await nextTick();
  markReady(props.meta, ["mounted", "vue-structure-committed"]);
});
</script>

<template>
  <main id="smoke-root" data-framework="vue" :data-family="meta.family" :data-expanded="String(expanded)">
    <h1>{{ meta.title }}</h1>
    <section data-case-body>
      <p v-if="spec.variant === 0 && expanded" data-branch="present">The conditional branch is present.</p>
      <aside v-else-if="spec.variant === 1 && expanded" data-mode="expanded">Expanded details</aside>
      <template v-else-if="spec.variant === 2">
        <span>Alpha</span><b>Beta</b><i>Gamma</i><span>Delta</span>
      </template>
      <template v-else-if="spec.variant === 3" data-kind="card">
        <article><h2>Template content</h2><p>Retained subtree</p></article>
      </template>
      <div v-else-if="spec.variant === 4" data-comment-host><!--vue-marker--><span>After marker</span></div>
      <details v-else-if="spec.variant === 5" :open="expanded"><summary>Compatibility details</summary><p>{{ deterministicWords(spec.seed, 12) }}</p></details>
      <dl v-else-if="spec.variant === 6"><div><dt>Engine</dt><dd>Moli</dd></div><div><dt>Reference</dt><dd>Chromium</dd></div></dl>
      <article v-else-if="spec.variant === 7"><header><h2>Semantic article</h2></header><nav aria-label="Article"><a href="#intro">Intro</a></nav><section id="intro"><p>Body</p></section><footer>End</footer></article>
      <div v-else-if="spec.variant === 8" data-depth="14"><div data-depth="13"><div data-depth="12"><div data-depth="11"><div data-depth="10"><div data-depth="9"><div data-depth="8"><div data-depth="7"><div data-depth="6"><div data-depth="5"><div data-depth="4"><div data-depth="3"><div data-depth="2"><div data-depth="1"><strong id="deep-leaf">Deep leaf {{ spec.seed }}</strong></div></div></div></div></div></div></div></div></div></div></div></div></div></div>
      <template v-else><section v-for="section in sections" :key="section.id" :aria-labelledby="section.id"><h2 :id="section.id">{{ section.title }}</h2><p>{{ section.text }}</p></section></template>
    </section>
  </main>
</template>

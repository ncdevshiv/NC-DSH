<script setup lang="ts">
import { computed, defineComponent, h, inject, nextTick, onMounted, provide, ref } from "vue";

import { stableItems, type StableItem } from "../shared/data";
import { captureFrame, markReady } from "../shared/harness";
import type { CaseSpec, SmokeMeta } from "../shared/types";

const props = defineProps<{ meta: SmokeMeta; spec: CaseSpec }>();
const active = ref(0);
const items = stableItems(props.spec.seed, Math.max(5, props.spec.size));
provide("theme", "ocean");
const contextTheme = inject("theme", "unset");
const activeLabel = computed(() => ["Overview", "Network", "Runtime"][active.value]);
const Badge = defineComponent({ props: { label: { type: String, required: true } }, setup(componentProps) { return () => h("span", { class: "badge" }, componentProps.label); } });
const TreeNode = defineComponent({
  name: "TreeNode",
  props: { item: { type: Object as () => StableItem, required: true }, depth: { type: Number, required: true } },
  setup(componentProps) {
    return () => h("li", { "data-depth": componentProps.depth }, [
      h("span", componentProps.item.title),
      componentProps.depth < 3 ? h("ul", [h(TreeNode, { item: { ...componentProps.item, id: `${componentProps.item.id}-${componentProps.depth}` }, depth: componentProps.depth + 1 })]) : null,
    ]);
  },
});
onMounted(async () => {
  await captureFrame(props.meta, "mounted");
  active.value = (props.spec.variant + 1) % 3;
  await nextTick();
  markReady(props.meta, ["mounted", "vue-component-composition"]);
});
</script>

<template>
  <main id="smoke-root" data-framework="vue" :data-family="meta.family">
    <h1>{{ meta.title }}</h1>
    <section data-case-body>
      <Badge v-if="spec.variant === 0" :label="`Input value ${spec.seed}`" />
      <article v-else-if="spec.variant === 1"><h2>Parent</h2><section><h3>Child</h3><Badge label="Grandchild" /></section></article>
      <article v-else-if="spec.variant === 2" :data-theme="contextTheme"><h2>Provide consumer</h2></article>
      <section v-else-if="spec.variant === 3"><header><Badge label="Header slot" /></header><div><p v-for="item in items.slice(0, 3)" :key="item.id">{{ item.title }}</p></div><footer>Footer slot</footer></section>
      <component :is="active === 1 ? 'article' : 'aside'" v-else-if="spec.variant === 4" :data-component="active === 1 ? 'alpha' : 'beta'">{{ active === 1 ? "Alpha component" : "Beta component" }}</component>
      <ul v-else-if="spec.variant === 5"><TreeNode :item="items[0]" :depth="0" /></ul>
      <section v-else-if="spec.variant === 6"><p>Local content</p><Teleport to="#portal-host"><aside id="portal-content">Portal {{ spec.seed }}</aside></Teleport></section>
      <div v-else-if="spec.variant === 7" role="group"><Badge label="Prefix" /><button>Action</button><Badge label="Suffix" /></div>
      <section v-else-if="spec.variant === 8"><div role="tablist"><button v-for="(label, index) in ['Overview', 'Network', 'Runtime']" :key="label" role="tab" :aria-selected="active === index">{{ label }}</button></div><article role="tabpanel"><h2>{{ activeLabel }}</h2><p>Selected panel {{ active }}</p></article></section>
      <div v-else class="app-shell"><header><h2>Moli Console</h2><Badge label="online" /></header><nav><a v-for="item in items.slice(0, 5)" :key="item.id" :href="`#${item.id}`">{{ item.title }}</a></nav><section><article v-for="item in items.slice(5)" :key="item.id"><h3>{{ item.title }}</h3><p>{{ item.owner }}</p></article></section><footer>Build {{ spec.seed }}</footer></div>
    </section>
  </main>
  <div id="portal-host"></div>
</template>

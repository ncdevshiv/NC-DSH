<script setup lang="ts">
import { computed, nextTick, onMounted, ref } from "vue";

import { money, stableItems } from "../shared/data";
import { captureFrame, markReady } from "../shared/harness";
import type { CaseSpec, SmokeMeta } from "../shared/types";

const props = defineProps<{ meta: SmokeMeta; spec: CaseSpec }>();
const source = stableItems(props.spec.seed, props.spec.variant === 9 ? 48 : Math.max(5, props.spec.size));
const items = ref([...source]);
const statuses = ["new", "active", "paused", "done"];
const groups = computed(() => statuses.map((status) => ({ status, items: items.value.filter((item) => item.status === status) })));
onMounted(async () => {
  await captureFrame(props.meta, "mounted");
  if (props.spec.variant === 1) items.value = [...source].reverse();
  else if (props.spec.variant === 2) items.value = [{ ...source[0], id: `prepended-${props.spec.seed}`, title: "Prepended checkpoint" }, ...source];
  else if (props.spec.variant === 3) items.value = source.filter((_, index) => index % 2 === 0);
  else items.value = source.map((item, index) => index === 1 ? { ...item, status: "done" } : item);
  await nextTick();
  markReady(props.meta, ["mounted", "vue-keyed-collection-update"]);
});
</script>

<template>
  <main id="smoke-root" data-framework="vue" :data-family="meta.family">
    <h1>{{ meta.title }}</h1>
    <section data-case-body>
      <ul v-if="spec.variant <= 3" :data-count="items.length"><li v-for="item in items" :key="item.id" :data-id="item.id" :data-status="item.status"><strong>{{ item.title }}</strong><span>{{ item.owner }}</span></li></ul>
      <template v-else-if="spec.variant === 4"><section v-for="group in groups" :key="group.status"><h2>{{ group.status }}</h2><ul><li v-for="item in group.items" :key="item.id"><strong>{{ item.title }}</strong><span>{{ item.owner }}</span></li></ul></section></template>
      <ul v-else-if="spec.variant === 5"><li v-for="(item, index) in items.slice(0, 6)" :key="item.id"><span>{{ item.title }}</span><ul v-if="index < 3"><li>{{ item.owner }}</li><li>{{ item.tags.join(" / ") }}</li></ul></li></ul>
      <table v-else-if="spec.variant === 6 || spec.variant === 9"><thead><tr><th>Item</th><th>Owner</th><th>Status</th><th>Amount</th></tr></thead><tbody><tr v-for="item in items" :key="item.id"><th scope="row">{{ item.title }}</th><td>{{ item.owner }}</td><td>{{ item.status }}</td><td>{{ money(item.amount) }}</td></tr></tbody></table>
      <div v-else-if="spec.variant === 7" role="list" class="card-grid"><article v-for="item in items" :key="item.id" role="listitem"><h2>{{ item.title }}</h2><p>{{ item.owner }}</p><div><span v-for="tag in item.tags" :key="tag">{{ tag }}</span></div></article></div>
      <ol v-else class="activity-feed"><li v-for="(item, index) in items" :key="item.id"><time :datetime="`2026-07-${String(index + 1).padStart(2, '0')}`">Day {{ index + 1 }}</time><p><b>{{ item.owner }}</b> moved {{ item.title }} to {{ item.status }}</p></li></ol>
    </section>
  </main>
</template>

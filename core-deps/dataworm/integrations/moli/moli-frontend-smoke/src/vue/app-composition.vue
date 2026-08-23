<script setup lang="ts">
import { computed, nextTick, onMounted, ref } from "vue";

import { deterministicWords, money, stableItems } from "../shared/data";
import { captureFrame, markReady } from "../shared/harness";
import type { CaseSpec, SmokeMeta } from "../shared/types";

const props = defineProps<{ meta: SmokeMeta; spec: CaseSpec }>();
const allItems = stableItems(props.spec.seed, 24);
const filter = ref("all");
const page = ref(0);
const visible = computed(() => {
  const selected = allItems.filter((item) => filter.value === "all" || item.status === filter.value);
  return selected.length ? selected : allItems.slice(0, 6);
});
onMounted(async () => {
  await captureFrame(props.meta, "mounted");
  filter.value = props.spec.variant % 2 === 0 ? "active" : "done";
  page.value = 1;
  await nextTick();
  markReady(props.meta, ["mounted", "vue-composed-app-update"]);
});
</script>

<template>
  <main id="smoke-root" data-framework="vue" :data-family="meta.family" :data-filter="filter">
    <h1>{{ meta.title }}</h1>
    <section data-case-body>
      <template v-if="spec.variant === 0"><header><a class="brand" href="#home">Moli</a><nav aria-label="Primary"><a href="#overview">Overview</a><a href="#activity">Activity</a><a href="#settings">Settings</a></nav><button>Account</button></header><section><h2>Welcome</h2><p>{{ deterministicWords(spec.seed, 24) }}</p></section></template>
      <div v-else-if="spec.variant === 1" class="sidebar-layout"><aside><h2>Workspace</h2><a v-for="item in allItems.slice(0, 8)" :key="item.id" :href="`#${item.id}`">{{ item.title }}</a></aside><section><h2>Selected view</h2>{{ deterministicWords(spec.seed, 40) }}</section></div>
      <div v-else-if="spec.variant === 2" class="stats-grid"><article v-for="(item, index) in allItems.slice(0, 8)" :key="item.id"><h2>{{ item.title }}</h2><strong>{{ item.amount + index * 17 }}</strong><small>{{ item.status }}</small></article></div>
      <template v-else-if="spec.variant === 3"><label>Search <input :value="filter" readonly></label><table><thead><tr><th>Name</th><th>Owner</th><th>Status</th></tr></thead><tbody><tr v-for="item in visible" :key="item.id"><td>{{ item.title }}</td><td>{{ item.owner }}</td><td>{{ item.status }}</td></tr></tbody></table></template>
      <template v-else-if="spec.variant === 4"><section><article v-for="item in allItems.slice(page * 6, page * 6 + 6)" :key="item.id"><h2>{{ item.title }}</h2></article></section><nav aria-label="Pages"><button v-for="value in [1, 2, 3, 4]" :key="value" :aria-current="page + 1 === value ? 'page' : undefined">{{ value }}</button></nav></template>
      <template v-else-if="spec.variant === 5"><div class="chips"><button v-for="value in ['all', 'new', 'active', 'done']" :key="value" :aria-pressed="filter === value">{{ value }}</button></div><ul><li v-for="item in visible" :key="item.id">{{ item.title }}</li></ul></template>
      <article v-else-if="spec.variant === 6" class="profile"><header><div aria-hidden="true">ML</div><div><h2>Moli Light</h2><p>Browser runtime engineer</p></div></header><dl><dt>Projects</dt><dd>18</dd><dt>Open reviews</dt><dd>7</dd><dt>Compatibility</dt><dd>92%</dd></dl><section><h3>About</h3><p>{{ deterministicWords(spec.seed, 60) }}</p></section></article>
      <section v-else-if="spec.variant === 7" class="notifications"><header><h2>Notifications</h2><button>Mark all read</button></header><article v-for="(item, index) in allItems.slice(0, 12)" :key="item.id" :data-unread="index < 4"><strong>{{ item.owner }}</strong><p>{{ item.title }}</p><time>09:{{ String(index * 4).padStart(2, "0") }}</time></article></section>
      <form v-else-if="spec.variant === 8" class="settings"><nav><button type="button">General</button><button type="button">Network</button><button type="button">Privacy</button></nav><section><h2>General settings</h2><label>Workspace name <input value="Moli Lab"></label><label>Theme <select value="system"><option value="system">System</option></select></label><label v-for="(label, index) in ['Tracing', 'Caching', 'Diagnostics']" :key="label"><input type="checkbox" :checked="index !== 1">{{ label }}</label></section></form>
      <div v-else class="admin"><header><h2>Administration</h2><button>Add member</button></header><section class="stats"><article v-for="item in allItems.slice(0, 4)" :key="item.id"><span>{{ item.status }}</span><strong>{{ money(item.amount) }}</strong></article></section><table><thead><tr><th>User</th><th>Role</th><th>Status</th><th>Actions</th></tr></thead><tbody><tr v-for="item in allItems.slice(0, 16)" :key="item.id"><td>{{ item.owner }}</td><td>{{ item.tags[0] }}</td><td>{{ item.status }}</td><td><button>Edit</button></td></tr></tbody></table></div>
    </section>
  </main>
</template>

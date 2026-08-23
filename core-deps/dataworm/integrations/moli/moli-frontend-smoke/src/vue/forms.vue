<script setup lang="ts">
import { nextTick, onMounted, ref } from "vue";

import { captureFrame, markReady } from "../shared/harness";
import type { CaseSpec, SmokeMeta } from "../shared/types";

const props = defineProps<{ meta: SmokeMeta; spec: CaseSpec }>();
const name = ref("Initial");
const notes = ref("Draft");
const checked = ref(["dom"]);
const priority = ref("normal");
const status = ref("new");
const regions = ref(["eu"]);
onMounted(async () => {
  await captureFrame(props.meta, "mounted");
  name.value = `User ${props.spec.seed}`;
  notes.value = `Updated notes ${props.spec.variant}`;
  checked.value = ["dom", "runtime"];
  priority.value = "high";
  status.value = "done";
  regions.value = ["us", "apac"];
  await nextTick();
  markReady(props.meta, ["mounted", "vue-model-form-update"]);
});
</script>

<template>
  <main id="smoke-root" data-framework="vue" :data-family="meta.family">
    <h1>{{ meta.title }}</h1>
    <section data-case-body>
      <label v-if="spec.variant === 0">Name <input v-model="name"></label>
      <label v-else-if="spec.variant === 1">Notes <textarea v-model="notes"></textarea></label>
      <fieldset v-else-if="spec.variant === 2"><legend>Scopes</legend><label v-for="value in ['dom', 'runtime', 'network']" :key="value"><input v-model="checked" type="checkbox" :value="value">{{ value }}</label></fieldset>
      <fieldset v-else-if="spec.variant === 3"><legend>Priority</legend><label v-for="value in ['low', 'normal', 'high']" :key="value"><input v-model="priority" type="radio" :value="value">{{ value }}</label></fieldset>
      <label v-else-if="spec.variant === 4">Status <select v-model="status"><option value="new">New</option><option value="active">Active</option><option value="done">Done</option></select></label>
      <label v-else-if="spec.variant === 5">Regions <select v-model="regions" multiple><option value="eu">Europe</option><option value="us">Americas</option><option value="apac">Asia Pacific</option></select></label>
      <div v-else-if="spec.variant === 6"><input v-model="name" readonly><input value="locked" disabled><button disabled>Save</button></div>
      <label v-else-if="spec.variant === 7">Email <input type="email" :value="`${name.replace(' ', '.').toLowerCase()}@example.test`" readonly :aria-invalid="false"><small role="status">Valid address</small></label>
      <fieldset v-else-if="spec.variant === 8"><legend>Profile editor</legend><label>Name <input v-model="name"></label><label>Notes <textarea v-model="notes"></textarea></label><label>Status <select v-model="status"><option value="done">Done</option></select></label></fieldset>
      <form v-else><h2>Checkout</h2><label>Customer <input v-model="name"></label><label>Delivery <select v-model="priority"><option value="high">Express</option></select></label><fieldset><legend>Extras</legend><label v-for="value in ['dom', 'runtime', 'network']" :key="value"><input v-model="checked" type="checkbox" :value="value">{{ value }}</label></fieldset><output>Total fields: 6</output><button type="submit">Place order</button></form>
    </section>
  </main>
</template>

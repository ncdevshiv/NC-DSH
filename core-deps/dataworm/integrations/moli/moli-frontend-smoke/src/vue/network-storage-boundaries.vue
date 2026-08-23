<script setup lang="ts">
import { nextTick, onMounted, ref } from "vue";

import { assertFixture, captureFrame, markReady } from "../shared/harness";
import {
  runNetworkStorageBoundaryCase,
  type NetworkStorageBoundaryResult,
} from "../shared/network-storage-boundaries";
import type { CaseSpec, SmokeMeta } from "../shared/types";

const props = defineProps<{ meta: SmokeMeta; spec: CaseSpec }>();
const host = ref<HTMLElement>();
const result = ref<NetworkStorageBoundaryResult>();

onMounted(async () => {
  await captureFrame(props.meta, "mounted");
  assertFixture(host.value, "Vue network/storage host exists");
  result.value = await runNetworkStorageBoundaryCase(
    host.value,
    props.meta,
    props.spec,
    (name) => captureFrame(props.meta, name),
  );
  await nextTick();
  markReady(props.meta, ["mounted", "vue-network-storage-ready"]);
});
</script>

<template>
  <main
    id="smoke-root"
    data-framework="vue"
    :data-family="meta.family"
    :data-mode="result?.status ?? 'loading'"
  >
    <h1>{{ meta.title }}</h1>
    <section data-case-body>
      <div ref="host" data-platform-host></div>
      <dl v-if="result" data-platform-facts>
        <div v-for="item in result.facts" :key="item.name" :data-fact="item.name">
          <dt>{{ item.name }}</dt>
          <dd>{{ item.value }}</dd>
        </div>
      </dl>
    </section>
  </main>
</template>

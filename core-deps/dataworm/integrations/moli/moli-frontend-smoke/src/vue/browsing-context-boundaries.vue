<script setup lang="ts">
import { nextTick, onMounted, ref } from "vue";

import {
  runBrowsingContextBoundaryCase,
  type BrowsingContextBoundaryResult,
} from "../shared/browsing-context-boundaries";
import { assertFixture, captureFrame, markReady } from "../shared/harness";
import type { CaseSpec, SmokeMeta } from "../shared/types";

const props = defineProps<{ meta: SmokeMeta; spec: CaseSpec }>();
const host = ref<HTMLElement>();
const result = ref<BrowsingContextBoundaryResult>();

onMounted(async () => {
  await captureFrame(props.meta, "mounted");
  assertFixture(host.value, "Vue browsing-context host exists");
  result.value = await runBrowsingContextBoundaryCase(host.value, props.meta, props.spec, (name) =>
    captureFrame(props.meta, name),
  );
  await nextTick();
  markReady(props.meta, ["mounted", "vue-browsing-context-ready"]);
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
      <div ref="host" data-boundary-host></div>
      <dl v-if="result" data-boundary-facts>
        <div v-for="item in result.facts" :key="item.name" :data-fact="item.name">
          <dt>{{ item.name }}</dt>
          <dd>{{ item.value }}</dd>
        </div>
      </dl>
    </section>
  </main>
</template>

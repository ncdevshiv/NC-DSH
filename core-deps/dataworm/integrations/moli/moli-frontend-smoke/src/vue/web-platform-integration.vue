<script setup lang="ts">
import { nextTick, onMounted, ref } from "vue";

import { assertFixture, captureFrame, markReady } from "../shared/harness";
import type { CaseSpec, SmokeMeta } from "../shared/types";
import {
  runWebPlatformIntegrationCase,
  type WebPlatformIntegrationResult,
} from "../shared/web-platform-integration";

const props = defineProps<{ meta: SmokeMeta; spec: CaseSpec }>();
const host = ref<HTMLElement>();
const result = ref<WebPlatformIntegrationResult>();

onMounted(async () => {
  await captureFrame(props.meta, "mounted");
  assertFixture(host.value, "Vue web-platform host exists");
  result.value = await runWebPlatformIntegrationCase(host.value, props.meta, props.spec);
  await nextTick();
  markReady(props.meta, ["mounted", "vue-web-platform-ready"]);
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

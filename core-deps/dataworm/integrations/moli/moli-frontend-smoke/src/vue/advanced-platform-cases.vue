<script setup lang="ts">
import { nextTick, onMounted, ref } from "vue";

import {
  runAdvancedPlatformCase,
  type AdvancedPlatformResult,
} from "../shared/advanced-platform-cases";
import { assertFixture, captureFrame, failCase, markReady } from "../shared/harness";
import type { CaseSpec, SmokeMeta } from "../shared/types";

const props = defineProps<{ meta: SmokeMeta; spec: CaseSpec }>();
const host = ref<HTMLElement>();
const result = ref<AdvancedPlatformResult>();

async function run(): Promise<void> {
  await captureFrame(props.meta, "mounted");
  assertFixture(host.value, "Vue advanced platform host exists");
  result.value = await runAdvancedPlatformCase(
    host.value,
    props.meta,
    props.spec,
    (name) => captureFrame(props.meta, name),
  );
  await nextTick();
  markReady(props.meta, ["mounted", "vue-advanced-platform-ready"]);
}

onMounted(() => {
  void run().catch(failCase);
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
        <div v-for="item of result.facts" :key="item.name" :data-fact="item.name">
          <dt>{{ item.name }}</dt>
          <dd>{{ item.value }}</dd>
        </div>
      </dl>
    </section>
  </main>
</template>

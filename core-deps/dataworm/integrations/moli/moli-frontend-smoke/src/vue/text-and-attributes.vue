<script setup lang="ts">
import { computed, nextTick, onMounted, ref } from "vue";

import { deterministicWords, stableItems } from "../shared/data";
import { assertFixture, captureFrame, markReady } from "../shared/harness";
import type { CaseSpec, SmokeMeta } from "../shared/types";

const props = defineProps<{ meta: SmokeMeta; spec: CaseSpec }>();
const updated = ref(false);
const items = computed(() => stableItems(props.spec.seed, Math.min(props.spec.size, 12)));

onMounted(async () => {
  await captureFrame(props.meta, "mounted");
  updated.value = true;
  await nextTick();
  assertFixture(document.querySelector("[data-case-body]") !== null, "case body mounted");
  markReady(props.meta, ["mounted", "vue-next-tick-update"]);
});
</script>

<template>
  <main
    id="smoke-root"
    data-framework="vue"
    :data-family="meta.family"
    :data-updated="String(updated)"
  >
    <h1>{{ meta.title }}</h1>
    <section data-case-body>
      <p v-if="spec.variant === 0" :data-value="spec.seed">
        Hello {{ spec.title }}; value {{ spec.seed * 3 }}
      </p>
      <p v-else-if="spec.variant === 1">
        {{ "<strong>literal & safe</strong> \"quoted\" 'single'" }}
      </p>
      <button
        v-else-if="spec.variant === 2"
        :disabled="!updated"
        :hidden="false"
        :required="updated"
      >
        Boolean projection
      </button>
      <section
        v-else-if="spec.variant === 3"
        :aria-label="`Panel ${spec.seed}`"
        :aria-busy="!updated"
        :data-seed="spec.seed"
      >
        ARIA dataset
      </section>
      <div
        v-else-if="spec.variant === 4"
        :class="updated ? ['card', 'active', 'selected'] : ['card', 'pending']"
      >
        Class tokens
      </div>
      <div
        v-else-if="spec.variant === 5"
        :style="{
          color: updated ? 'rgb(12, 34, 56)' : 'black',
          marginTop: `${spec.variant + 2}px`,
        }"
      >
        Style map
      </div>
      <p v-else-if="spec.variant === 6" lang="zh-Hans" dir="auto">
        你好，世界 — مرحبا — café — 😀 — {{ updated ? "更新" : "初始" }}
      </p>
      <dl v-else-if="spec.variant === 7">
        <dt>Present</dt>
        <dd>{{ updated ? "value" : null }}</dd>
        <dt>Missing</dt>
        <dd>{{ undefined }}</dd>
      </dl>
      <dl v-else-if="spec.variant === 8">
        <div v-for="item in items" :key="item.id">
          <dt>{{ item.title }}</dt>
          <dd :data-status="item.status">{{ item.owner }}</dd>
        </div>
      </dl>
      <article v-else>
        <header>
          <p class="eyebrow">Engineering / Browser</p>
          <h2>{{ spec.title }}</h2>
          <p>{{ deterministicWords(spec.seed, 28) }}</p>
        </header>
        <div>
          <span v-for="item in items" :key="item.id" :data-tag="item.tags[0]">
            {{ item.title }}
          </span>
        </div>
      </article>
    </section>
  </main>
</template>

<script setup lang="ts">
import {
  computed,
  defineComponent,
  h,
  nextTick,
  onMounted,
  ref,
  type PropType,
} from "vue";

import {
  createGalleryViewModel,
  createGalleryViewState,
  galleryCardCount,
  galleryCardHidden,
  galleryFacts,
  galleryMoney,
  galleryOrderedItems,
  galleryPrimaryState,
  gallerySecondaryState,
  gallerySentence,
  galleryTableCount,
  type GalleryItem,
  type GalleryViewModel,
  type GalleryViewState,
} from "../shared/gallery-cases";
import { assertFixture, captureFrame, markReady } from "../shared/harness";
import type { CaseSpec, SmokeMeta } from "../shared/types";

const props = defineProps<{ meta: SmokeMeta; spec: CaseSpec }>();
const model = createGalleryViewModel(props.meta, props.spec);
const state = ref(createGalleryViewState());
const primaryButton = ref<HTMLButtonElement>();
const secondaryButton = ref<HTMLButtonElement>();
const facts = computed(() => galleryFacts(model, state.value));

const GalleryCard = defineComponent({
  props: {
    model: { type: Object as PropType<GalleryViewModel>, required: true },
    state: { type: Object as PropType<GalleryViewState>, required: true },
    item: { type: Object as PropType<GalleryItem>, required: true },
    index: { type: Number, required: true },
    action: { type: String, required: true },
  },
  setup(cardProps) {
    return () => {
      const { item, index } = cardProps;
      return h(
        "article",
        {
          "data-gallery-card": "",
          "data-selectable": "",
          "data-item-id": item.id,
          "data-rank":
            cardProps.state.phase === "ready"
              ? (index * 7 + cardProps.model.spec.seed) % 31
              : undefined,
          "aria-current": cardProps.state.selectedId === item.id ? "true" : "false",
          hidden: galleryCardHidden(cardProps.model, cardProps.state, item, index) || undefined,
        },
        [
          h("header", [
            h("span", { class: "eyebrow" }, item.status),
            h("time", { datetime: item.date }, item.date),
          ]),
          h("h3", item.title),
          h("p", item.summary),
          h("ul", { "aria-label": "Tags" }, item.tags.map((tag) => h("li", { key: tag }, tag))),
          h("footer", [
            h("span", item.owner),
            h("strong", galleryMoney(item.amount)),
            h("button", { type: "button", "data-card-action": index }, cardProps.action),
          ]),
        ],
      );
    };
  },
});

const GalleryCards = defineComponent({
  props: {
    model: { type: Object as PropType<GalleryViewModel>, required: true },
    state: { type: Object as PropType<GalleryViewState>, required: true },
    action: { type: String, required: true },
  },
  setup(cardProps) {
    return () =>
      h(
        "div",
        { class: "gallery-card-grid", "data-reorder-list": "" },
        galleryOrderedItems(
          cardProps.model,
          cardProps.state,
          0,
          galleryCardCount(cardProps.model),
        ).map((item, index) =>
          h(GalleryCard, {
            key: item.id,
            model: cardProps.model,
            state: cardProps.state,
            item,
            index,
            action: cardProps.action,
          }),
        ),
      );
  },
});

const GalleryTable = defineComponent({
  props: {
    model: { type: Object as PropType<GalleryViewModel>, required: true },
    state: { type: Object as PropType<GalleryViewState>, required: true },
    caption: { type: String, required: true },
    start: { type: Number, default: 8 },
  },
  setup(tableProps) {
    return () =>
      h("table", { "data-gallery-table": "" }, [
        h("caption", tableProps.caption),
        h("thead", [
          h("tr", [
            h("th", { scope: "col" }, "Item"),
            h("th", { scope: "col" }, "Owner"),
            h("th", { scope: "col" }, "Status"),
            h("th", { scope: "col" }, "Value"),
            h("th", { scope: "col" }, "Date"),
          ]),
        ]),
        h(
          "tbody",
          { "data-reorder-table": "" },
          galleryOrderedItems(
            tableProps.model,
            tableProps.state,
            tableProps.start,
            galleryTableCount(tableProps.model),
          ).map((item) =>
            h("tr", { key: item.id, "data-row-id": item.id }, [
              h("th", { scope: "row" }, item.title),
              h("td", item.owner),
              h("td", [h("span", { "data-status": item.status }, item.status)]),
              h("td", galleryMoney(item.amount)),
              h("td", [h("time", { datetime: item.date }, item.date)]),
            ]),
          ),
        ),
      ]);
  },
});

function primary(): void {
  state.value = galleryPrimaryState(model, state.value);
}

function secondary(): void {
  state.value = gallerySecondaryState(model, state.value);
}

function submit(event: Event): void {
  event.preventDefault();
}

onMounted(async () => {
  const surface = document.querySelector("[data-gallery-surface]");
  assertFixture(surface, "Vue gallery surface exists");
  assertFixture(
    surface.querySelectorAll("*").length >= 120,
    `${props.meta.id} has a substantial Vue tree`,
  );
  assertFixture(
    surface.querySelectorAll("[data-gallery-card]").length >= 12,
    `${props.meta.id} has Vue-rendered cards`,
  );
  await captureFrame(props.meta, "mounted");
  assertFixture(primaryButton.value, "Vue primary gallery action exists");
  primaryButton.value.click();
  await nextTick();
  await captureFrame(props.meta, "interaction-1");
  assertFixture(secondaryButton.value, "Vue secondary gallery action exists");
  secondaryButton.value.click();
  await nextTick();
  markReady(props.meta, ["mounted", "interaction-1", "vue-gallery-ready"]);
});
</script>

<template>
  <main
    id="smoke-root"
    data-framework="vue"
    :data-family="meta.family"
    :data-mode="state.phase"
  >
    <header data-framework-gallery-header>
      <div>
        <p>Gallery-inspired complex application</p>
        <h1>{{ meta.title }}</h1>
      </div>
      <div role="group" aria-label="Deterministic gallery transitions">
        <button
          ref="primaryButton"
          type="button"
          data-gallery-primary-action
          :aria-pressed="state.phase !== 'mounted'"
          @click="primary"
        >Focus and reorder</button>
        <button
          ref="secondaryButton"
          type="button"
          data-gallery-secondary-action
          :aria-pressed="state.phase === 'ready'"
          @click="secondary"
        >Filter and finalize</button>
      </div>
    </header>

    <section data-case-body>
      <div
        data-gallery-surface
        :data-gallery-family="model.family"
        :data-gallery-slug="model.spec.slug"
        :data-phase="state.phase"
        :data-ready="state.phase === 'ready' ? 'true' : undefined"
      >
        <header data-gallery-header>
          <a href="#gallery-top" data-brand>{{ model.profile.brand }}</a>
          <nav aria-label="Primary">
            <a
              v-for="(label, index) in model.navLabels"
              :key="label"
              :href="`#section-${index}`"
              :aria-current="index === 0 ? 'page' : undefined"
            >{{ label }}</a>
          </nav>
          <form role="search" data-search-form @submit="submit">
            <label><span>Search</span><input
              type="search"
              name="query"
              :value="state.query"
              autocomplete="off"
              data-gallery-query
              readonly
            ></label>
            <button type="submit">Search</button>
          </form>
          <button type="button" aria-haspopup="menu">Account</button>
        </header>

        <section
          id="gallery-top"
          aria-labelledby="gallery-headline"
          data-gallery-hero
          :data-focused="state.phase !== 'mounted' ? '' : undefined"
        >
          <p class="kicker">{{ model.profile.kicker }}</p>
          <h2 id="gallery-headline">{{ model.profile.headline }}</h2>
          <p>{{ gallerySentence(model.spec.seed + model.spec.variant, 34) }}</p>
          <div class="hero-actions">
            <a href="#section-1">Explore the work</a>
            <button
              type="button"
              :aria-expanded="state.phase !== 'mounted'"
              data-expand-control
            >View details</button>
          </div>
          <ul aria-label="Highlights">
            <li v-for="item in model.items.slice(0, 4)" :key="item.id">
              <strong>{{ item.percent }}%</strong><span>{{ item.title }}</span>
            </li>
          </ul>
        </section>

        <template v-if="model.family === 'gallery-marketing'">
          <section id="section-1" aria-labelledby="trusted-title">
            <h2 id="trusted-title">Selected partners</h2>
            <ul class="logo-wall"><li v-for="item in model.items.slice(0, 8)" :key="item.id">{{ item.owner }}</li></ul>
          </section>
          <section id="section-2" aria-labelledby="capabilities-title">
            <header><p>Capabilities</p><h2 id="capabilities-title">A system built from connected parts</h2></header>
            <GalleryCards :model="model" :state="state" action="Read case" />
          </section>
          <section class="story" aria-labelledby="story-title">
            <figure><div role="img" :aria-label="`${model.profile.brand} field study`">Study {{ model.spec.variant + 1 }}</div><figcaption>{{ model.items[20].owner }} documented four working sessions.</figcaption></figure>
            <article><p class="kicker">Featured field report</p><h2 id="story-title">{{ model.items[20].title }}</h2>
              <section v-for="(item, index) in model.items.slice(21, 25)" :id="`story-part-${index + 1}`" :key="item.id"><h3>{{ index + 1 }}. {{ item.title }}</h3><p>{{ gallerySentence(model.spec.seed + index * 7, 28) }}</p></section>
            </article>
          </section>
          <section id="section-3" aria-labelledby="plans-title"><h2 id="plans-title">Choose an operating model</h2><div class="plan-grid">
            <article v-for="(item, index) in model.items.slice(16, 20)" :key="item.id" :data-plan="index"><h3>{{ ["Starter", "Team", "Scale", "Enterprise"][index] }}</h3><p>{{ item.summary }}</p><strong>{{ galleryMoney(item.amount) }} / month</strong><ul><li v-for="feature in model.items.slice(index, index + 4)" :key="feature.id">{{ feature.title }}</li></ul><button type="button">Choose plan</button></article>
          </div></section>
          <section id="section-4" aria-labelledby="faq-title"><h2 id="faq-title">Questions and operating details</h2>
            <details v-for="(item, index) in model.items.slice(24, 30)" :key="item.id" :open="index === 0 || (index === 1 && state.phase !== 'mounted')"><summary>{{ item.title }}</summary><p>{{ item.summary }} {{ gallerySentence(model.spec.seed + index, 14) }}</p></details>
          </section>
        </template>

        <template v-else-if="model.family === 'gallery-commerce'">
          <div class="commerce-layout" id="section-1">
            <aside aria-label="Catalog filters"><h2>Refine selection</h2><form data-filter-form @submit="submit"><fieldset><legend>Availability</legend><label v-for="(label, index) in ['Ready to ship', 'Preorder', 'Local pickup']" :key="label"><input type="checkbox" :checked="index === 0">{{ label }}</label></fieldset><label>Sort<select :value="state.phase === 'ready' ? 'newest' : 'featured'"><option value="featured">Featured</option><option value="price">Price</option><option value="newest">Newest</option></select></label><label>Maximum<input type="number" min="10" max="900" :value="320 + state.controlValue"></label><label v-if="model.spec.variant === 9">Auction window<progress max="100" :value="state.controlValue">{{ state.controlValue }}%</progress></label></form></aside>
            <section aria-labelledby="products-title"><header><h2 id="products-title">Available now</h2><output>{{ galleryCardCount(model) }} results</output></header><GalleryCards :model="model" :state="state" action="Add to order" /></section>
            <aside aria-labelledby="cart-title"><h2 id="cart-title">Current order</h2><ol><li v-for="item in model.items.slice(18, 23)" :key="item.id"><span>{{ item.title }}</span><strong>{{ galleryMoney(item.amount) }}</strong></li></ol><dl><dt>Subtotal</dt><dd>{{ galleryMoney(model.items.slice(18, 23).reduce((total, item) => total + item.amount, 0)) }}</dd><dt>Delivery</dt><dd>$24.00</dd></dl><button type="button">Review checkout</button></aside>
          </div>
          <section id="section-2" aria-labelledby="comparison-title"><h2 id="comparison-title">Detailed comparison</h2><GalleryTable :model="model" :state="state" caption="Selected products" :start="6" /></section>
          <section id="section-3" aria-labelledby="reviews-title"><h2 id="reviews-title">Recent verified reviews</h2><blockquote v-for="item in model.items.slice(20, 28)" :key="item.id"><p>{{ item.summary }}</p><footer><cite>{{ item.owner }}</cite> · {{ item.percent }}/100</footer></blockquote></section>
        </template>

        <template v-else-if="model.family === 'gallery-editorial'">
          <nav aria-label="Topics" id="section-1"><a v-for="item in model.items.slice(0, 9)" :key="item.id" :href="`#${item.id}`">{{ item.tags[0] }}</a></nav>
          <section class="lead-package" aria-labelledby="lead-title"><article><p class="kicker">{{ model.items[20].status }}</p><h2 id="lead-title">{{ model.items[20].title }}</h2><p>{{ gallerySentence(model.spec.seed, 52) }}</p><p>By {{ model.items[20].owner }}</p></article><figure><div role="img" :aria-label="`${model.items[20].title} illustration`">01</div><figcaption>{{ model.items[20].summary }}</figcaption></figure><aside aria-label="Editors' brief"><article v-for="item in model.items.slice(21, 25)" :key="item.id"><h3>{{ item.title }}</h3><p>{{ item.summary }}</p></article></aside></section>
          <section id="section-2" aria-labelledby="stories-title"><h2 id="stories-title">Latest stories</h2><GalleryCards :model="model" :state="state" action="Save story" /></section>
          <section class="live-desk" aria-labelledby="live-title"><header><h2 id="live-title">Live desk</h2><progress v-if="model.spec.variant === 4" max="90" :value="Math.min(90, state.controlValue)">Live progress</progress></header><ol><li v-for="(item, index) in model.items.slice(14, 24)" :key="item.id"><time :datetime="`${item.date}T${9 + index}:00:00Z`">{{ 9 + index }}:00</time><h3>{{ item.title }}</h3><p>{{ item.summary }}</p></li></ol></section>
          <section id="section-3" aria-labelledby="archive-title"><h2 id="archive-title">Browse the archive</h2><GalleryTable :model="model" :state="state" caption="Publication archive" :start="12" /></section>
          <form id="section-4" data-newsletter @submit="submit"><h2>Get the weekly edition</h2><label>Email<input type="email" :value="`reader+${state.phase}@example.test`"></label><label><input type="checkbox" :checked="state.phase !== 'mounted'">Include the weekend reading list</label><button type="submit">Subscribe</button></form>
        </template>

        <template v-else-if="model.family === 'gallery-workspaces'">
          <div class="workspace-shell" id="section-1">
            <aside><h2>Workspace</h2><nav aria-label="Workspace"><a v-for="(item, index) in model.items.slice(0, 10)" :key="item.id" :href="`#${item.id}`" :aria-current="index === 0 ? 'page' : undefined">{{ item.title }}</a></nav><button type="button">Create new</button></aside>
            <section class="workspace-main"><header><div><p class="kicker">Current workspace</p><h2>{{ model.profile.headline }}</h2></div><div><button type="button">Share</button><button type="button">More</button></div></header><div class="metrics"><article v-for="item in model.items.slice(0, 6)" :key="item.id"><span>{{ item.title }}</span><strong>{{ item.amount }}</strong><small>{{ item.percent }}% · {{ item.status }}</small></article></div><div class="board" data-reorder-list><section v-for="(column, columnIndex) in ['Planned', 'Active', 'Review', 'Complete']" :key="column" :aria-labelledby="`column-${columnIndex}`"><header><h3 :id="`column-${columnIndex}`">{{ column }}</h3><span>5</span></header><GalleryCard v-for="(item, index) in galleryOrderedItems(model, state, 0, 20).slice(columnIndex * 5, columnIndex * 5 + 5)" :key="item.id" :model="model" :state="state" :item="item" :index="columnIndex * 5 + index" action="Inspect" /></section></div></section>
            <aside aria-label="Activity"><h2>Recent activity</h2><ol><li v-for="item in model.items.slice(20, 30)" :key="item.id"><strong>{{ item.owner }}</strong><span>{{ item.title }}</span><time :datetime="item.date">{{ item.date }}</time></li></ol></aside>
          </div>
          <section id="section-2" aria-labelledby="records-title"><h2 id="records-title">Records and assignments</h2><GalleryTable :model="model" :state="state" caption="Workspace records" :start="8" /></section>
          <article class="workspace-editor" aria-labelledby="editor-title"><h2 id="editor-title">{{ model.items[24].title }}</h2><div contenteditable="true" role="textbox" aria-multiline="true" data-editor><p>{{ gallerySentence(model.spec.seed, 34) }}</p><h3>Decision record</h3><ul><li v-for="item in model.items.slice(25, 29)" :key="item.id">{{ item.title }}</li></ul><p>{{ gallerySentence(model.spec.seed + 7, 26) }}</p></div></article>
          <dialog :open="state.phase !== 'mounted'"><h2>Share workspace</h2><p>{{ model.items[30].summary }}</p><button type="button">Copy link</button></dialog>
        </template>

        <template v-else-if="model.family === 'gallery-operations'">
          <section class="operations-overview" id="section-1"><header><div><p class="kicker">{{ model.profile.kicker }}</p><h2>{{ model.profile.headline }}</h2></div><label>Window<select :value="state.phase === 'ready' ? 'week' : 'day'"><option value="day">Last 24 hours</option><option value="week">Last 7 days</option></select></label></header>
            <div class="metrics"><article v-for="(item, index) in model.items.slice(0, 8)" :key="item.id"><span>{{ item.title }}</span><strong :data-summary-value="index === 0 ? '' : undefined">{{ index === 0 && state.phase === 'ready' ? item.amount + model.items[1].percent : item.amount }}</strong><small>{{ item.percent }}% · {{ item.status }}</small></article></div>
            <meter v-if="model.spec.variant === 4" min="0" max="100" low="40" high="85" optimum="90" :value="state.controlValue">Grid health</meter>
            <svg viewBox="0 0 720 240" role="img" aria-labelledby="operations-chart-title"><title id="operations-chart-title">Operational activity by interval</title><g><rect v-for="(item, index) in model.items.slice(0, 24)" :key="item.id" :x="index * 29 + 10" :y="210 - item.percent * 2" width="18" :height="item.percent * 2" :data-bar="index" /></g><polyline fill="none" stroke="currentColor" :points="model.items.slice(0, 24).map((item, index) => `${index * 29 + 19},${210 - ((item.percent + state.controlValue) % 90) * 2}`).join(' ')" /></svg>
          </section>
          <div class="operations-grid" id="section-2"><section aria-labelledby="signals-title"><h2 id="signals-title">Signals requiring attention</h2><GalleryCards :model="model" :state="state" action="Review" /></section><aside aria-labelledby="responders-title"><h2 id="responders-title">Owners on rotation</h2><article v-for="item in model.items.slice(12, 20)" :key="item.id"><strong>{{ item.owner }}</strong><span>{{ item.status }}</span><small>{{ item.tags.join(" · ") }}</small></article></aside></div>
          <section id="section-3" aria-labelledby="resources-title"><h2 id="resources-title">Resources and controls</h2><GalleryTable :model="model" :state="state" caption="Operational resources" :start="5" /></section>
          <section id="section-4" aria-labelledby="timeline-title"><h2 id="timeline-title">Decision timeline</h2><ol><li v-for="(item, index) in model.items.slice(8, 20)" :key="item.id"><time :datetime="`${item.date}T${8 + index}:00:00Z`">{{ 8 + index }}:00</time><h3>{{ item.title }}</h3><p>{{ item.summary }}</p><span>{{ item.owner }}</span></li></ol></section>
        </template>

        <template v-else-if="model.family === 'gallery-community'">
          <section class="community-profile" id="section-1" aria-labelledby="community-title"><div role="img" :aria-label="`${model.profile.brand} cover`">{{ model.profile.brand.slice(0, 2) }}</div><div><p class="kicker">{{ model.profile.kicker }}</p><h2 id="community-title">{{ model.profile.headline }}</h2><p>{{ gallerySentence(model.spec.seed, 30) }}</p><button type="button">Join community</button><button type="button">Share</button></div><dl><dt>Members</dt><dd>{{ model.items[0].amount }}</dd><dt>Groups</dt><dd>{{ model.items[1].percent }}</dd><dt>Events</dt><dd>{{ model.items[2].percent }}</dd></dl></section>
          <nav aria-label="Community sections"><button v-for="(label, index) in ['Highlights', 'Discussions', 'Projects', 'Events', 'Members']" :key="label" type="button" role="tab" :aria-selected="(state.phase === 'ready' ? 2 : 0) === index">{{ label }}</button></nav>
          <div class="community-layout" id="section-2"><main><form data-composer @submit="submit"><label>Start a discussion<textarea rows="3" :value="`Share an ${state.phase} update with the community`"></textarea></label><button type="submit">Publish</button></form><section aria-labelledby="feed-title"><h2 id="feed-title">Community feed</h2><GalleryCards :model="model" :state="state" action="Respond" /></section></main>
            <aside aria-labelledby="people-title"><h2 id="people-title">People to meet</h2><article v-for="item in model.items.slice(15, 23)" :key="item.id"><span aria-hidden="true">{{ item.owner.slice(0, 1) }}</span><h3>{{ item.owner }}</h3><p>{{ item.title }}</p><button type="button">Connect</button></article><fieldset><legend>Weekly poll</legend><label v-for="(label, index) in ['Morning', 'Afternoon', 'Evening']" :key="label"><input type="radio" name="poll" :value="label" :checked="(state.phase === 'ready' ? 2 : 1) === index">{{ label }}</label><button type="button">Vote</button></fieldset></aside>
          </div>
          <section id="section-3" aria-labelledby="events-title"><h2 id="events-title">Upcoming sessions</h2><GalleryTable :model="model" :state="state" caption="Community schedule" :start="6" /></section>
          <section id="section-4" aria-labelledby="guidelines-title"><h2 id="guidelines-title">Guidelines and resources</h2><details v-for="(item, index) in model.items.slice(23, 28)" :key="item.id" :open="index === 0 || (index === 1 && state.phase !== 'mounted')"><summary>{{ item.title }}</summary><p>{{ item.summary }}</p></details></section>
        </template>

        <template v-else>
          <section id="section-1" class="service-search" aria-labelledby="service-search-title"><h2 id="service-search-title">Plan the next step</h2><form data-service-search @submit="submit">
            <div><label>From<input name="from" :value="model.spec.variant < 2 ? 'Shanghai' : 'Primary account'"></label><label>To<input name="to" :value="state.phase === 'ready' ? 'Selected service' : 'Recommended service'"></label></div>
            <div><label>Start date<input type="date" name="start" value="2026-09-18"></label><label>End date<input type="date" name="end" value="2026-09-22"></label></div>
            <label>People<input type="number" min="1" max="12" :value="state.phase === 'mounted' ? 2 : 3"></label><label>Preference<select :value="state.phase === 'ready' ? 'flexible' : 'balanced'"><optgroup label="Recommended"><option value="balanced">Balanced</option><option value="fastest">Fastest</option></optgroup><optgroup label="Flexible"><option value="flexible">Most flexible</option></optgroup></select></label>
            <template v-if="model.spec.variant === 0"><label>Airport<input list="airport-options" value="Shanghai"></label><datalist id="airport-options"><option value="Shanghai"></option><option value="Tokyo"></option><option value="Seoul"></option></datalist></template>
            <label v-else-if="model.spec.variant === 2">Transfer allocation<input type="range" min="0" max="100" :value="state.controlValue"></label>
            <label v-else-if="model.spec.variant === 3">Claim completion<progress max="100" :value="state.controlValue">{{ state.controlValue }}%</progress></label>
            <label v-else-if="model.spec.variant === 4">Care plan adherence<meter min="0" max="100" low="40" high="85" optimum="90" :value="state.controlValue">{{ state.controlValue }}%</meter></label>
            <button type="submit">Find options</button>
          </form></section>
          <div class="service-layout" id="section-2"><section aria-labelledby="options-title"><header><h2 id="options-title">Recommended options</h2><output>{{ galleryCardCount(model) }} available</output></header><GalleryCards :model="model" :state="state" action="Select" /></section><aside aria-labelledby="summary-title"><h2 id="summary-title">Current selection</h2><ol><li v-for="(step, index) in ['Profile', 'Options', 'Details', 'Review', 'Complete']" :key="step" :aria-current="index === (state.phase === 'ready' ? 2 : 1) ? 'step' : undefined"><span>{{ index + 1 }}</span>{{ step }}</li></ol><h3>{{ model.items[0].title }}</h3><p>{{ model.items[0].summary }}</p><dl><dt>Estimated value</dt><dd>{{ galleryMoney(model.items[0].amount) }}</dd><dt>Owner</dt><dd>{{ model.items[0].owner }}</dd><dt>Status</dt><dd>{{ state.phase }}</dd></dl><button type="button">Continue</button></aside></div>
          <section id="section-3" aria-labelledby="documents-title"><h2 id="documents-title">Documents and recent activity</h2><GalleryTable :model="model" :state="state" caption="Service records" :start="8" /></section>
          <section id="section-4" aria-labelledby="help-title"><h2 id="help-title">Help for this process</h2><details v-for="(item, index) in model.items.slice(18, 24)" :key="item.id" :open="index === 0 || (index === 1 && state.phase !== 'mounted')"><summary>{{ item.title }}</summary><p>{{ item.summary }}</p></details></section>
        </template>

        <section aria-live="polite" aria-atomic="false" data-live-region>
          <h2>Live page updates</h2>
          <output data-live-status>{{ state.phase === "mounted" ? "Waiting for interaction" : `${state.phase} with ${galleryCardCount(model)} interactive records` }}</output>
          <div data-dynamic-region>
            <article v-for="sequence in state.dynamicSequences" :key="sequence" data-dynamic-card :data-dynamic-sequence="sequence"><p class="kicker">Live update</p><h2>{{ model.items[31].title }} · {{ sequence }}</h2><p>{{ model.items[31].summary }}</p><span>{{ model.items[31].owner }}</span></article>
          </div>
        </section>
        <component :is="'template'" data-update-template>
          <article data-dynamic-card data-dynamic-sequence="template"><p class="kicker">Live update</p><h2>{{ model.items[31].title }}</h2><p>{{ model.items[31].summary }}</p><span>{{ model.items[31].owner }}</span></article>
        </component>
        <footer data-gallery-footer><nav aria-label="Footer"><a href="#gallery-top">Back to top</a><a href="#privacy">Privacy</a><a href="#accessibility">Accessibility</a></nav><p>© 2026 {{ model.profile.brand }}</p></footer>
      </div>

      <dl data-gallery-facts>
        <div v-for="fact in facts" :key="fact.name" :data-fact="fact.name">
          <dt>{{ fact.name }}</dt><dd>{{ fact.value }}</dd>
        </div>
      </dl>
    </section>
  </main>
</template>

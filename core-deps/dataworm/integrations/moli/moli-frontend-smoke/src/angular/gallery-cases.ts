import {
  Component,
  computed,
  signal,
  type Type,
} from "@angular/core";

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
import { assertFixture, captureFrame } from "../shared/harness";
import type { CaseSpec, SmokeMeta } from "../shared/types";
import { bootstrapAngular, settleAngular } from "./support";

@Component({
  selector: "gallery-cards",
  standalone: true,
  inputs: ["model", "state", "action"],
  template: `
    <div class="gallery-card-grid" data-reorder-list>
      @for (item of cards(); track item.id; let index = $index) {
        <article
          data-gallery-card
          data-selectable
          [attr.data-item-id]="item.id"
          [attr.data-rank]="state.phase === 'ready' ? (index * 7 + model.spec.seed) % 31 : null"
          [attr.aria-current]="state.selectedId === item.id ? 'true' : 'false'"
          [attr.hidden]="hidden(item, index) ? '' : null"
        >
          <header>
            <span class="eyebrow">{{ item.status }}</span>
            <time [attr.datetime]="item.date">{{ item.date }}</time>
          </header>
          <h3>{{ item.title }}</h3>
          <p>{{ item.summary }}</p>
          <ul aria-label="Tags">
            @for (tag of item.tags; track tag) { <li>{{ tag }}</li> }
          </ul>
          <footer>
            <span>{{ item.owner }}</span>
            <strong>{{ money(item.amount) }}</strong>
            <button type="button" [attr.data-card-action]="index">{{ action }}</button>
          </footer>
        </article>
      }
    </div>
  `,
})
class AngularGalleryCards {
  model!: GalleryViewModel;
  state!: GalleryViewState;
  action = "";
  readonly money = galleryMoney;

  cards(): GalleryItem[] {
    return galleryOrderedItems(this.model, this.state, 0, galleryCardCount(this.model));
  }

  hidden(item: GalleryItem, index: number): boolean {
    return galleryCardHidden(this.model, this.state, item, index);
  }
}

@Component({
  selector: "gallery-table",
  standalone: true,
  inputs: ["model", "state", "caption", "start"],
  template: `
    <table data-gallery-table>
      <caption>{{ caption }}</caption>
      <thead>
        <tr>
          <th scope="col">Item</th><th scope="col">Owner</th><th scope="col">Status</th>
          <th scope="col">Value</th><th scope="col">Date</th>
        </tr>
      </thead>
      <tbody data-reorder-table>
        @for (item of rows(); track item.id) {
          <tr [attr.data-row-id]="item.id">
            <th scope="row">{{ item.title }}</th>
            <td>{{ item.owner }}</td>
            <td><span [attr.data-status]="item.status">{{ item.status }}</span></td>
            <td>{{ money(item.amount) }}</td>
            <td><time [attr.datetime]="item.date">{{ item.date }}</time></td>
          </tr>
        }
      </tbody>
    </table>
  `,
})
class AngularGalleryTable {
  model!: GalleryViewModel;
  state!: GalleryViewState;
  caption = "";
  start = 8;
  readonly money = galleryMoney;

  rows(): GalleryItem[] {
    return galleryOrderedItems(
      this.model,
      this.state,
      this.start,
      galleryTableCount(this.model),
    );
  }
}

interface GalleryComponent {
  readonly model: GalleryViewModel;
  readonly state: ReturnType<typeof signal<GalleryViewState>>;
  primary(): void;
  secondary(): void;
}

function componentType(meta: SmokeMeta, spec: CaseSpec): Type<GalleryComponent> {
  class AngularGalleryComponent implements GalleryComponent {
    readonly meta = meta;
    readonly spec = spec;
    readonly model = createGalleryViewModel(meta, spec);
    readonly state = signal(createGalleryViewState());
    readonly facts = computed(() => galleryFacts(this.model, this.state()));
    readonly cardCount = galleryCardCount;
    readonly orderedItems = galleryOrderedItems;
    readonly money = galleryMoney;
    readonly sentence = gallerySentence;
    readonly Math = Math;
    readonly totalAmount = (total: number, item: GalleryItem): number => total + item.amount;

    chartPoints(): string {
      return this.model.items
        .slice(0, 24)
        .map(
          (item, index) =>
            `${index * 29 + 19},${
              210 - ((item.percent + this.state().controlValue) % 90) * 2
            }`,
        )
        .join(" ");
    }

    primary(): void {
      this.state.update((current) => galleryPrimaryState(this.model, current));
    }

    secondary(): void {
      this.state.update((current) => gallerySecondaryState(this.model, current));
    }

    submit(event: Event): void {
      event.preventDefault();
    }

    ngAfterViewInit(): void {
      const surface = document.querySelector("[data-gallery-surface]");
      assertFixture(surface, "Angular gallery surface exists");
      assertFixture(
        surface.querySelectorAll("*").length >= 120,
        `${meta.id} has a substantial Angular tree`,
      );
      assertFixture(
        surface.querySelectorAll("[data-gallery-card]").length >= 12,
        `${meta.id} has Angular-rendered cards`,
      );
    }
  }

  Component({
    selector: "smoke-root",
    standalone: true,
    imports: [AngularGalleryCards, AngularGalleryTable],
    template: `
      <main
        id="smoke-root"
        data-framework="angular"
        [attr.data-family]="meta.family"
        [attr.data-mode]="state().phase"
      >
        <header data-framework-gallery-header>
          <div>
            <p>Gallery-inspired complex application</p>
            <h1>{{ meta.title }}</h1>
          </div>
          <div role="group" aria-label="Deterministic gallery transitions">
            <button
              type="button"
              data-gallery-primary-action
              [attr.aria-pressed]="state().phase !== 'mounted'"
              (click)="primary()"
            >Focus and reorder</button>
            <button
              type="button"
              data-gallery-secondary-action
              [attr.aria-pressed]="state().phase === 'ready'"
              (click)="secondary()"
            >Filter and finalize</button>
          </div>
        </header>

        <section data-case-body>
          <div
            data-gallery-surface
            [attr.data-gallery-family]="model.family"
            [attr.data-gallery-slug]="model.spec.slug"
            [attr.data-phase]="state().phase"
            [attr.data-ready]="state().phase === 'ready' ? 'true' : null"
          >
            <header data-gallery-header>
              <a href="#gallery-top" data-brand>{{ model.profile.brand }}</a>
              <nav aria-label="Primary">
                @for (label of model.navLabels; track label; let index = $index) {
                  <a
                    [href]="'#section-' + index"
                    [attr.aria-current]="index === 0 ? 'page' : null"
                  >{{ label }}</a>
                }
              </nav>
              <form role="search" data-search-form (submit)="submit($event)">
                <label>
                  <span>Search</span>
                  <input
                    type="search"
                    name="query"
                    [value]="state().query"
                    autocomplete="off"
                    data-gallery-query
                    readonly
                  >
                </label>
                <button type="submit">Search</button>
              </form>
              <button type="button" aria-haspopup="menu">Account</button>
            </header>

            <section
              id="gallery-top"
              aria-labelledby="gallery-headline"
              data-gallery-hero
              [attr.data-focused]="state().phase !== 'mounted' ? '' : null"
            >
              <p class="kicker">{{ model.profile.kicker }}</p>
              <h2 id="gallery-headline">{{ model.profile.headline }}</h2>
              <p>{{ sentence(model.spec.seed + model.spec.variant, 34) }}</p>
              <div class="hero-actions">
                <a href="#section-1">Explore the work</a>
                <button
                  type="button"
                  [attr.aria-expanded]="state().phase !== 'mounted'"
                  data-expand-control
                >View details</button>
              </div>
              <ul aria-label="Highlights">
                @for (item of model.items.slice(0, 4); track item.id) {
                  <li><strong>{{ item.percent }}%</strong><span>{{ item.title }}</span></li>
                }
              </ul>
            </section>

            @switch (model.family) {
              @case ("gallery-marketing") {
                <section id="section-1" aria-labelledby="trusted-title">
                  <h2 id="trusted-title">Selected partners</h2>
                  <ul class="logo-wall">
                    @for (item of model.items.slice(0, 8); track item.id) {
                      <li>{{ item.owner }}</li>
                    }
                  </ul>
                </section>
                <section id="section-2" aria-labelledby="capabilities-title">
                  <header>
                    <p>Capabilities</p>
                    <h2 id="capabilities-title">A system built from connected parts</h2>
                  </header>
                  <gallery-cards [model]="model" [state]="state()" action="Read case" />
                </section>
                <section class="story" aria-labelledby="story-title">
                  <figure>
                    <div role="img" [attr.aria-label]="model.profile.brand + ' field study'">
                      Study {{ model.spec.variant + 1 }}
                    </div>
                    <figcaption>{{ model.items[20].owner }} documented four working sessions.</figcaption>
                  </figure>
                  <article>
                    <p class="kicker">Featured field report</p>
                    <h2 id="story-title">{{ model.items[20].title }}</h2>
                    @for (item of model.items.slice(21, 25); track item.id; let index = $index) {
                      <section [id]="'story-part-' + (index + 1)">
                        <h3>{{ index + 1 }}. {{ item.title }}</h3>
                        <p>{{ sentence(model.spec.seed + index * 7, 28) }}</p>
                      </section>
                    }
                  </article>
                </section>
                <section id="section-3" aria-labelledby="plans-title">
                  <h2 id="plans-title">Choose an operating model</h2>
                  <div class="plan-grid">
                    @for (item of model.items.slice(16, 20); track item.id; let index = $index) {
                      <article [attr.data-plan]="index">
                        <h3>{{ ["Starter", "Team", "Scale", "Enterprise"][index] }}</h3>
                        <p>{{ item.summary }}</p><strong>{{ money(item.amount) }} / month</strong>
                        <ul>
                          @for (feature of model.items.slice(index, index + 4); track feature.id) {
                            <li>{{ feature.title }}</li>
                          }
                        </ul>
                        <button type="button">Choose plan</button>
                      </article>
                    }
                  </div>
                </section>
                <section id="section-4" aria-labelledby="faq-title">
                  <h2 id="faq-title">Questions and operating details</h2>
                  @for (item of model.items.slice(24, 30); track item.id; let index = $index) {
                    <details [open]="index === 0 || (index === 1 && state().phase !== 'mounted')">
                      <summary>{{ item.title }}</summary>
                      <p>{{ item.summary }} {{ sentence(model.spec.seed + index, 14) }}</p>
                    </details>
                  }
                </section>
              }

              @case ("gallery-commerce") {
                <div class="commerce-layout" id="section-1">
                  <aside aria-label="Catalog filters">
                    <h2>Refine selection</h2>
                    <form data-filter-form (submit)="submit($event)">
                      <fieldset>
                        <legend>Availability</legend>
                        @for (label of ["Ready to ship", "Preorder", "Local pickup"]; track label; let index = $index) {
                          <label><input type="checkbox" [checked]="index === 0">{{ label }}</label>
                        }
                      </fieldset>
                      <label>Sort
                        <select [value]="state().phase === 'ready' ? 'newest' : 'featured'">
                          <option value="featured">Featured</option>
                          <option value="price">Price</option>
                          <option value="newest">Newest</option>
                        </select>
                      </label>
                      <label>Maximum
                        <input type="number" min="10" max="900" [value]="320 + state().controlValue">
                      </label>
                      @if (model.spec.variant === 9) {
                        <label>Auction window
                          <progress max="100" [value]="state().controlValue">
                            {{ state().controlValue }}%
                          </progress>
                        </label>
                      }
                    </form>
                  </aside>
                  <section aria-labelledby="products-title">
                    <header>
                      <h2 id="products-title">Available now</h2>
                      <output>{{ cardCount(model) }} results</output>
                    </header>
                    <gallery-cards [model]="model" [state]="state()" action="Add to order" />
                  </section>
                  <aside aria-labelledby="cart-title">
                    <h2 id="cart-title">Current order</h2>
                    <ol>
                      @for (item of model.items.slice(18, 23); track item.id) {
                        <li><span>{{ item.title }}</span><strong>{{ money(item.amount) }}</strong></li>
                      }
                    </ol>
                    <dl>
                      <dt>Subtotal</dt><dd>{{ money(model.items.slice(18, 23).reduce(totalAmount, 0)) }}</dd>
                      <dt>Delivery</dt><dd>$24.00</dd>
                    </dl>
                    <button type="button">Review checkout</button>
                  </aside>
                </div>
                <section id="section-2" aria-labelledby="comparison-title">
                  <h2 id="comparison-title">Detailed comparison</h2>
                  <gallery-table [model]="model" [state]="state()" caption="Selected products" [start]="6" />
                </section>
                <section id="section-3" aria-labelledby="reviews-title">
                  <h2 id="reviews-title">Recent verified reviews</h2>
                  @for (item of model.items.slice(20, 28); track item.id) {
                    <blockquote><p>{{ item.summary }}</p><footer><cite>{{ item.owner }}</cite> · {{ item.percent }}/100</footer></blockquote>
                  }
                </section>
              }

              @case ("gallery-editorial") {
                <nav aria-label="Topics" id="section-1">
                  @for (item of model.items.slice(0, 9); track item.id) {
                    <a [href]="'#' + item.id">{{ item.tags[0] }}</a>
                  }
                </nav>
                <section class="lead-package" aria-labelledby="lead-title">
                  <article><p class="kicker">{{ model.items[20].status }}</p><h2 id="lead-title">{{ model.items[20].title }}</h2><p>{{ sentence(model.spec.seed, 52) }}</p><p>By {{ model.items[20].owner }}</p></article>
                  <figure><div role="img" [attr.aria-label]="model.items[20].title + ' illustration'">01</div><figcaption>{{ model.items[20].summary }}</figcaption></figure>
                  <aside aria-label="Editors' brief">
                    @for (item of model.items.slice(21, 25); track item.id) {
                      <article><h3>{{ item.title }}</h3><p>{{ item.summary }}</p></article>
                    }
                  </aside>
                </section>
                <section id="section-2" aria-labelledby="stories-title">
                  <h2 id="stories-title">Latest stories</h2>
                  <gallery-cards [model]="model" [state]="state()" action="Save story" />
                </section>
                <section class="live-desk" aria-labelledby="live-title">
                  <header>
                    <h2 id="live-title">Live desk</h2>
                    @if (model.spec.variant === 4) {
                      <progress max="90" [value]="Math.min(90, state().controlValue)">Live progress</progress>
                    }
                  </header>
                  <ol>
                    @for (item of model.items.slice(14, 24); track item.id; let index = $index) {
                      <li><time [attr.datetime]="item.date + 'T' + (9 + index) + ':00:00Z'">{{ 9 + index }}:00</time><h3>{{ item.title }}</h3><p>{{ item.summary }}</p></li>
                    }
                  </ol>
                </section>
                <section id="section-3" aria-labelledby="archive-title">
                  <h2 id="archive-title">Browse the archive</h2>
                  <gallery-table [model]="model" [state]="state()" caption="Publication archive" [start]="12" />
                </section>
                <form id="section-4" data-newsletter (submit)="submit($event)">
                  <h2>Get the weekly edition</h2>
                  <label>Email<input type="email" [value]="'reader+' + state().phase + '@example.test'"></label>
                  <label><input type="checkbox" [checked]="state().phase !== 'mounted'">Include the weekend reading list</label>
                  <button type="submit">Subscribe</button>
                </form>
              }

              @case ("gallery-workspaces") {
                <div class="workspace-shell" id="section-1">
                  <aside>
                    <h2>Workspace</h2>
                    <nav aria-label="Workspace">
                      @for (item of model.items.slice(0, 10); track item.id; let index = $index) {
                        <a [href]="'#' + item.id" [attr.aria-current]="index === 0 ? 'page' : null">{{ item.title }}</a>
                      }
                    </nav>
                    <button type="button">Create new</button>
                  </aside>
                  <section class="workspace-main">
                    <header><div><p class="kicker">Current workspace</p><h2>{{ model.profile.headline }}</h2></div><div><button type="button">Share</button><button type="button">More</button></div></header>
                    <div class="metrics">
                      @for (item of model.items.slice(0, 6); track item.id) {
                        <article><span>{{ item.title }}</span><strong>{{ item.amount }}</strong><small>{{ item.percent }}% · {{ item.status }}</small></article>
                      }
                    </div>
                    <div class="board" data-reorder-list>
                      <gallery-cards [model]="model" [state]="state()" action="Inspect" />
                    </div>
                  </section>
                  <aside aria-label="Activity">
                    <h2>Recent activity</h2>
                    <ol>
                      @for (item of model.items.slice(20, 30); track item.id) {
                        <li><strong>{{ item.owner }}</strong><span>{{ item.title }}</span><time [attr.datetime]="item.date">{{ item.date }}</time></li>
                      }
                    </ol>
                  </aside>
                </div>
                <section id="section-2" aria-labelledby="records-title">
                  <h2 id="records-title">Records and assignments</h2>
                  <gallery-table [model]="model" [state]="state()" caption="Workspace records" [start]="8" />
                </section>
                <article class="workspace-editor" aria-labelledby="editor-title">
                  <h2 id="editor-title">{{ model.items[24].title }}</h2>
                  <div contenteditable="true" role="textbox" aria-multiline="true" data-editor>
                    <p>{{ sentence(model.spec.seed, 34) }}</p><h3>Decision record</h3>
                    <ul>
                      @for (item of model.items.slice(25, 29); track item.id) { <li>{{ item.title }}</li> }
                    </ul>
                    <p>{{ sentence(model.spec.seed + 7, 26) }}</p>
                  </div>
                </article>
                <dialog [open]="state().phase !== 'mounted'">
                  <h2>Share workspace</h2><p>{{ model.items[30].summary }}</p><button type="button">Copy link</button>
                </dialog>
              }

              @case ("gallery-operations") {
                <section class="operations-overview" id="section-1">
                  <header>
                    <div><p class="kicker">{{ model.profile.kicker }}</p><h2>{{ model.profile.headline }}</h2></div>
                    <label>Window
                      <select [value]="state().phase === 'ready' ? 'week' : 'day'">
                        <option value="day">Last 24 hours</option><option value="week">Last 7 days</option>
                      </select>
                    </label>
                  </header>
                  <div class="metrics">
                    @for (item of model.items.slice(0, 8); track item.id; let index = $index) {
                      <article><span>{{ item.title }}</span><strong [attr.data-summary-value]="index === 0 ? '' : null">{{ index === 0 && state().phase === 'ready' ? item.amount + model.items[1].percent : item.amount }}</strong><small>{{ item.percent }}% · {{ item.status }}</small></article>
                    }
                  </div>
                  @if (model.spec.variant === 4) {
                    <meter min="0" max="100" low="40" high="85" optimum="90" [value]="state().controlValue">Grid health</meter>
                  }
                  <svg viewBox="0 0 720 240" role="img" aria-labelledby="operations-chart-title">
                    <title id="operations-chart-title">Operational activity by interval</title>
                    <g>
                      @for (item of model.items.slice(0, 24); track item.id; let index = $index) {
                        <rect [attr.x]="index * 29 + 10" [attr.y]="210 - item.percent * 2" width="18" [attr.height]="item.percent * 2" [attr.data-bar]="index" />
                      }
                    </g>
                    <polyline fill="none" stroke="currentColor" [attr.points]="chartPoints()" />
                  </svg>
                </section>
                <div class="operations-grid" id="section-2">
                  <section aria-labelledby="signals-title"><h2 id="signals-title">Signals requiring attention</h2><gallery-cards [model]="model" [state]="state()" action="Review" /></section>
                  <aside aria-labelledby="responders-title"><h2 id="responders-title">Owners on rotation</h2>
                    @for (item of model.items.slice(12, 20); track item.id) {
                      <article><strong>{{ item.owner }}</strong><span>{{ item.status }}</span><small>{{ item.tags.join(" · ") }}</small></article>
                    }
                  </aside>
                </div>
                <section id="section-3" aria-labelledby="resources-title">
                  <h2 id="resources-title">Resources and controls</h2>
                  <gallery-table [model]="model" [state]="state()" caption="Operational resources" [start]="5" />
                </section>
                <section id="section-4" aria-labelledby="timeline-title">
                  <h2 id="timeline-title">Decision timeline</h2>
                  <ol>
                    @for (item of model.items.slice(8, 20); track item.id; let index = $index) {
                      <li><time [attr.datetime]="item.date + 'T' + (8 + index) + ':00:00Z'">{{ 8 + index }}:00</time><h3>{{ item.title }}</h3><p>{{ item.summary }}</p><span>{{ item.owner }}</span></li>
                    }
                  </ol>
                </section>
              }

              @case ("gallery-community") {
                <section class="community-profile" id="section-1" aria-labelledby="community-title">
                  <div role="img" [attr.aria-label]="model.profile.brand + ' cover'">{{ model.profile.brand.slice(0, 2) }}</div>
                  <div><p class="kicker">{{ model.profile.kicker }}</p><h2 id="community-title">{{ model.profile.headline }}</h2><p>{{ sentence(model.spec.seed, 30) }}</p><button type="button">Join community</button><button type="button">Share</button></div>
                  <dl><dt>Members</dt><dd>{{ model.items[0].amount }}</dd><dt>Groups</dt><dd>{{ model.items[1].percent }}</dd><dt>Events</dt><dd>{{ model.items[2].percent }}</dd></dl>
                </section>
                <nav aria-label="Community sections">
                  @for (label of ["Highlights", "Discussions", "Projects", "Events", "Members"]; track label; let index = $index) {
                    <button type="button" role="tab" [attr.aria-selected]="(state().phase === 'ready' ? 2 : 0) === index">{{ label }}</button>
                  }
                </nav>
                <div class="community-layout" id="section-2">
                  <main>
                    <form data-composer (submit)="submit($event)"><label>Start a discussion<textarea rows="3" [value]="'Share an ' + state().phase + ' update with the community'"></textarea></label><button type="submit">Publish</button></form>
                    <section aria-labelledby="feed-title"><h2 id="feed-title">Community feed</h2><gallery-cards [model]="model" [state]="state()" action="Respond" /></section>
                  </main>
                  <aside aria-labelledby="people-title">
                    <h2 id="people-title">People to meet</h2>
                    @for (item of model.items.slice(15, 23); track item.id) {
                      <article><span aria-hidden="true">{{ item.owner.slice(0, 1) }}</span><h3>{{ item.owner }}</h3><p>{{ item.title }}</p><button type="button">Connect</button></article>
                    }
                    <fieldset><legend>Weekly poll</legend>
                      @for (label of ["Morning", "Afternoon", "Evening"]; track label; let index = $index) {
                        <label><input type="radio" name="poll" [value]="label" [checked]="(state().phase === 'ready' ? 2 : 1) === index">{{ label }}</label>
                      }
                      <button type="button">Vote</button>
                    </fieldset>
                  </aside>
                </div>
                <section id="section-3" aria-labelledby="events-title"><h2 id="events-title">Upcoming sessions</h2><gallery-table [model]="model" [state]="state()" caption="Community schedule" [start]="6" /></section>
                <section id="section-4" aria-labelledby="guidelines-title"><h2 id="guidelines-title">Guidelines and resources</h2>
                  @for (item of model.items.slice(23, 28); track item.id; let index = $index) {
                    <details [open]="index === 0 || (index === 1 && state().phase !== 'mounted')"><summary>{{ item.title }}</summary><p>{{ item.summary }}</p></details>
                  }
                </section>
              }

              @default {
                <section id="section-1" class="service-search" aria-labelledby="service-search-title">
                  <h2 id="service-search-title">Plan the next step</h2>
                  <form data-service-search (submit)="submit($event)">
                    <div><label>From<input name="from" [value]="model.spec.variant < 2 ? 'Shanghai' : 'Primary account'"></label><label>To<input name="to" [value]="state().phase === 'ready' ? 'Selected service' : 'Recommended service'"></label></div>
                    <div><label>Start date<input type="date" name="start" value="2026-09-18"></label><label>End date<input type="date" name="end" value="2026-09-22"></label></div>
                    <label>People<input type="number" min="1" max="12" [value]="state().phase === 'mounted' ? 2 : 3"></label>
                    <label>Preference
                      <select [value]="state().phase === 'ready' ? 'flexible' : 'balanced'">
                        <optgroup label="Recommended"><option value="balanced">Balanced</option><option value="fastest">Fastest</option></optgroup>
                        <optgroup label="Flexible"><option value="flexible">Most flexible</option></optgroup>
                      </select>
                    </label>
                    @switch (model.spec.variant) {
                      @case (0) {
                        <label>Airport<input list="airport-options" value="Shanghai"></label>
                        <datalist id="airport-options"><option value="Shanghai"></option><option value="Tokyo"></option><option value="Seoul"></option></datalist>
                      }
                      @case (2) {
                        <label>Transfer allocation<input type="range" min="0" max="100" [value]="state().controlValue"></label>
                      }
                      @case (3) {
                        <label>Claim completion<progress max="100" [value]="state().controlValue">{{ state().controlValue }}%</progress></label>
                      }
                      @case (4) {
                        <label>Care plan adherence<meter min="0" max="100" low="40" high="85" optimum="90" [value]="state().controlValue">{{ state().controlValue }}%</meter></label>
                      }
                    }
                    <button type="submit">Find options</button>
                  </form>
                </section>
                <div class="service-layout" id="section-2">
                  <section aria-labelledby="options-title"><header><h2 id="options-title">Recommended options</h2><output>{{ cardCount(model) }} available</output></header><gallery-cards [model]="model" [state]="state()" action="Select" /></section>
                  <aside aria-labelledby="summary-title">
                    <h2 id="summary-title">Current selection</h2>
                    <ol>
                      @for (step of ["Profile", "Options", "Details", "Review", "Complete"]; track step; let index = $index) {
                        <li [attr.aria-current]="index === (state().phase === 'ready' ? 2 : 1) ? 'step' : null"><span>{{ index + 1 }}</span>{{ step }}</li>
                      }
                    </ol>
                    <h3>{{ model.items[0].title }}</h3><p>{{ model.items[0].summary }}</p>
                    <dl><dt>Estimated value</dt><dd>{{ money(model.items[0].amount) }}</dd><dt>Owner</dt><dd>{{ model.items[0].owner }}</dd><dt>Status</dt><dd>{{ state().phase }}</dd></dl>
                    <button type="button">Continue</button>
                  </aside>
                </div>
                <section id="section-3" aria-labelledby="documents-title"><h2 id="documents-title">Documents and recent activity</h2><gallery-table [model]="model" [state]="state()" caption="Service records" [start]="8" /></section>
                <section id="section-4" aria-labelledby="help-title"><h2 id="help-title">Help for this process</h2>
                  @for (item of model.items.slice(18, 24); track item.id; let index = $index) {
                    <details [open]="index === 0 || (index === 1 && state().phase !== 'mounted')"><summary>{{ item.title }}</summary><p>{{ item.summary }}</p></details>
                  }
                </section>
              }
            }

            <section aria-live="polite" aria-atomic="false" data-live-region>
              <h2>Live page updates</h2>
              <output data-live-status>{{ state().phase === "mounted" ? "Waiting for interaction" : state().phase + " with " + cardCount(model) + " interactive records" }}</output>
              <div data-dynamic-region>
                @for (sequence of state().dynamicSequences; track sequence) {
                  <article data-dynamic-card [attr.data-dynamic-sequence]="sequence"><p class="kicker">Live update</p><h2>{{ model.items[31].title }} · {{ sequence }}</h2><p>{{ model.items[31].summary }}</p><span>{{ model.items[31].owner }}</span></article>
                }
              </div>
            </section>
            <template data-update-template>
              <article data-dynamic-card data-dynamic-sequence="template"><p class="kicker">Live update</p><h2>{{ model.items[31].title }}</h2><p>{{ model.items[31].summary }}</p><span>{{ model.items[31].owner }}</span></article>
            </template>
            <footer data-gallery-footer>
              <nav aria-label="Footer"><a href="#gallery-top">Back to top</a><a href="#privacy">Privacy</a><a href="#accessibility">Accessibility</a></nav>
              <p>© 2026 {{ model.profile.brand }}</p>
            </footer>
          </div>

          <dl data-gallery-facts>
            @for (fact of facts(); track fact.name) {
              <div [attr.data-fact]="fact.name"><dt>{{ fact.name }}</dt><dd>{{ fact.value }}</dd></div>
            }
          </dl>
        </section>
      </main>
    `,
  })(AngularGalleryComponent);
  return AngularGalleryComponent;
}

export async function mount(meta: SmokeMeta, spec: CaseSpec): Promise<void> {
  await bootstrapAngular(
    componentType(meta, spec),
    meta,
    "angular-gallery-ready",
    async (_instance, application) => {
      const primary = document.querySelector("[data-gallery-primary-action]");
      const secondary = document.querySelector("[data-gallery-secondary-action]");
      assertFixture(primary instanceof HTMLButtonElement, "Angular primary gallery action exists");
      assertFixture(
        secondary instanceof HTMLButtonElement,
        "Angular secondary gallery action exists",
      );
      primary.click();
      await settleAngular(application);
      await captureFrame(meta, "interaction-1");
      secondary.click();
    },
  );
}

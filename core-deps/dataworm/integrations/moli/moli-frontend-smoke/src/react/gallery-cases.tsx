import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type ReactNode,
} from "react";

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
import { assertFixture, captureFrame, failCase, markReady } from "../shared/harness";
import { mountReact, type ReactCaseProps } from "./support";

interface GalleryProps {
  model: GalleryViewModel;
  state: GalleryViewState;
}

function preventSubmit(event: FormEvent): void {
  event.preventDefault();
}

function GalleryCard({
  model,
  state,
  item,
  index,
  action,
}: GalleryProps & { item: GalleryItem; index: number; action: string }) {
  const hidden = galleryCardHidden(model, state, item, index);
  return (
    <article
      data-gallery-card=""
      data-selectable=""
      data-item-id={item.id}
      data-rank={state.phase === "ready" ? (index * 7 + model.spec.seed) % 31 : undefined}
      aria-current={state.selectedId === item.id ? "true" : "false"}
      hidden={hidden}
    >
      <header>
        <span className="eyebrow">{item.status}</span>
        <time dateTime={item.date}>{item.date}</time>
      </header>
      <h3>{item.title}</h3>
      <p>{item.summary}</p>
      <ul aria-label="Tags">
        {item.tags.map((tag) => <li key={tag}>{tag}</li>)}
      </ul>
      <footer>
        <span>{item.owner}</span>
        <strong>{galleryMoney(item.amount)}</strong>
        <button type="button" data-card-action={index}>{action}</button>
      </footer>
    </article>
  );
}

function GalleryCards({ model, state, action }: GalleryProps & { action: string }) {
  const items = galleryOrderedItems(model, state, 0, galleryCardCount(model));
  return (
    <div className="gallery-card-grid" data-reorder-list="">
      {items.map((item, index) => (
        <GalleryCard
          key={item.id}
          model={model}
          state={state}
          item={item}
          index={index}
          action={action}
        />
      ))}
    </div>
  );
}

function GalleryTable({
  model,
  state,
  caption,
  start = 8,
}: GalleryProps & { caption: string; start?: number }) {
  const rows = galleryOrderedItems(model, state, start, galleryTableCount(model));
  return (
    <table data-gallery-table="">
      <caption>{caption}</caption>
      <thead>
        <tr>
          <th scope="col">Item</th>
          <th scope="col">Owner</th>
          <th scope="col">Status</th>
          <th scope="col">Value</th>
          <th scope="col">Date</th>
        </tr>
      </thead>
      <tbody data-reorder-table="">
        {rows.map((item) => (
          <tr key={item.id} data-row-id={item.id}>
            <th scope="row">{item.title}</th>
            <td>{item.owner}</td>
            <td><span data-status={item.status}>{item.status}</span></td>
            <td>{galleryMoney(item.amount)}</td>
            <td><time dateTime={item.date}>{item.date}</time></td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function CommonHeader({ model, state }: GalleryProps) {
  return (
    <header data-gallery-header="">
      <a href="#gallery-top" data-brand="">{model.profile.brand}</a>
      <nav aria-label="Primary">
        {model.navLabels.map((label, index) => (
          <a key={label} href={`#section-${index}`} aria-current={index === 0 ? "page" : undefined}>
            {label}
          </a>
        ))}
      </nav>
      <form role="search" data-search-form="" onSubmit={preventSubmit}>
        <label>
          <span>Search</span>
          <input
            type="search"
            name="query"
            value={state.query}
            autoComplete="off"
            data-gallery-query=""
            readOnly
          />
        </label>
        <button type="submit">Search</button>
      </form>
      <button type="button" aria-haspopup="menu">Account</button>
    </header>
  );
}

function CommonHero({ model, state }: GalleryProps) {
  return (
    <section
      id="gallery-top"
      aria-labelledby="gallery-headline"
      data-gallery-hero=""
      data-focused={state.phase !== "mounted" ? "" : undefined}
    >
      <p className="kicker">{model.profile.kicker}</p>
      <h2 id="gallery-headline">{model.profile.headline}</h2>
      <p>{gallerySentence(model.spec.seed + model.spec.variant, 34)}</p>
      <div className="hero-actions">
        <a href="#section-1">Explore the work</a>
        <button
          type="button"
          aria-expanded={state.phase !== "mounted"}
          data-expand-control=""
        >
          View details
        </button>
      </div>
      <ul aria-label="Highlights">
        {model.items.slice(0, 4).map((item) => (
          <li key={item.id}><strong>{item.percent}%</strong><span>{item.title}</span></li>
        ))}
      </ul>
    </section>
  );
}

function Marketing({ model, state }: GalleryProps) {
  return (
    <>
      <section id="section-1" aria-labelledby="trusted-title">
        <h2 id="trusted-title">Selected partners</h2>
        <ul className="logo-wall">
          {model.items.slice(0, 8).map((item) => <li key={item.id}>{item.owner}</li>)}
        </ul>
      </section>
      <section id="section-2" aria-labelledby="capabilities-title">
        <header><p>Capabilities</p><h2 id="capabilities-title">A system built from connected parts</h2></header>
        <GalleryCards model={model} state={state} action="Read case" />
      </section>
      <section className="story" aria-labelledby="story-title">
        <figure>
          <div role="img" aria-label={`${model.profile.brand} field study`}>Study {model.spec.variant + 1}</div>
          <figcaption>{model.items[20].owner} documented four working sessions.</figcaption>
        </figure>
        <article>
          <p className="kicker">Featured field report</p>
          <h2 id="story-title">{model.items[20].title}</h2>
          {model.items.slice(21, 25).map((item, index) => (
            <section key={item.id} id={`story-part-${index + 1}`}>
              <h3>{index + 1}. {item.title}</h3>
              <p>{gallerySentence(model.spec.seed + index * 7, 28)}</p>
            </section>
          ))}
        </article>
      </section>
      <section id="section-3" aria-labelledby="plans-title">
        <h2 id="plans-title">Choose an operating model</h2>
        <div className="plan-grid">
          {model.items.slice(16, 20).map((item, index) => (
            <article key={item.id} data-plan={index}>
              <h3>{["Starter", "Team", "Scale", "Enterprise"][index]}</h3>
              <p>{item.summary}</p><strong>{galleryMoney(item.amount)} / month</strong>
              <ul>{model.items.slice(index, index + 4).map((feature) => <li key={feature.id}>{feature.title}</li>)}</ul>
              <button type="button">Choose plan</button>
            </article>
          ))}
        </div>
      </section>
      <section id="section-4" aria-labelledby="faq-title">
        <h2 id="faq-title">Questions and operating details</h2>
        {model.items.slice(24, 30).map((item, index) => (
          <details key={item.id} open={index === 0 || (index === 1 && state.phase !== "mounted")}>
            <summary>{item.title}</summary><p>{item.summary} {gallerySentence(model.spec.seed + index, 14)}</p>
          </details>
        ))}
      </section>
    </>
  );
}

function Commerce({ model, state }: GalleryProps) {
  return (
    <>
      <div className="commerce-layout" id="section-1">
        <aside aria-label="Catalog filters">
          <h2>Refine selection</h2>
          <form data-filter-form="" onSubmit={preventSubmit}>
            <fieldset>
              <legend>Availability</legend>
              {["Ready to ship", "Preorder", "Local pickup"].map((label, index) => (
                <label key={label}><input type="checkbox" checked={index === 0} readOnly />{label}</label>
              ))}
            </fieldset>
            <label>Sort<select value={state.phase === "ready" ? "newest" : "featured"} onChange={() => undefined}>
              <option value="featured">Featured</option><option value="price">Price</option><option value="newest">Newest</option>
            </select></label>
            <label>Maximum<input type="number" min="10" max="900" value={320 + state.controlValue} readOnly /></label>
            {model.spec.variant === 9 && <label>Auction window<progress max="100" value={state.controlValue}>{state.controlValue}%</progress></label>}
          </form>
        </aside>
        <section aria-labelledby="products-title">
          <header><h2 id="products-title">Available now</h2><output>{galleryCardCount(model)} results</output></header>
          <GalleryCards model={model} state={state} action="Add to order" />
        </section>
        <aside aria-labelledby="cart-title">
          <h2 id="cart-title">Current order</h2>
          <ol>{model.items.slice(18, 23).map((item) => <li key={item.id}><span>{item.title}</span><strong>{galleryMoney(item.amount)}</strong></li>)}</ol>
          <dl><dt>Subtotal</dt><dd>{galleryMoney(model.items.slice(18, 23).reduce((total, item) => total + item.amount, 0))}</dd><dt>Delivery</dt><dd>$24.00</dd></dl>
          <button type="button">Review checkout</button>
        </aside>
      </div>
      <section id="section-2" aria-labelledby="comparison-title">
        <h2 id="comparison-title">Detailed comparison</h2>
        <GalleryTable model={model} state={state} caption="Selected products" start={6} />
      </section>
      <section id="section-3" aria-labelledby="reviews-title">
        <h2 id="reviews-title">Recent verified reviews</h2>
        {model.items.slice(20, 28).map((item) => <blockquote key={item.id}><p>{item.summary}</p><footer><cite>{item.owner}</cite> · {item.percent}/100</footer></blockquote>)}
      </section>
    </>
  );
}

function Editorial({ model, state }: GalleryProps) {
  return (
    <>
      <nav aria-label="Topics" id="section-1">
        {model.items.slice(0, 9).map((item) => <a key={item.id} href={`#${item.id}`}>{item.tags[0]}</a>)}
      </nav>
      <section className="lead-package" aria-labelledby="lead-title">
        <article><p className="kicker">{model.items[20].status}</p><h2 id="lead-title">{model.items[20].title}</h2><p>{gallerySentence(model.spec.seed, 52)}</p><p>By {model.items[20].owner}</p></article>
        <figure><div role="img" aria-label={`${model.items[20].title} illustration`}>01</div><figcaption>{model.items[20].summary}</figcaption></figure>
        <aside aria-label="Editors' brief">{model.items.slice(21, 25).map((item) => <article key={item.id}><h3>{item.title}</h3><p>{item.summary}</p></article>)}</aside>
      </section>
      <section id="section-2" aria-labelledby="stories-title">
        <h2 id="stories-title">Latest stories</h2><GalleryCards model={model} state={state} action="Save story" />
      </section>
      <section className="live-desk" aria-labelledby="live-title">
        <header><h2 id="live-title">Live desk</h2>{model.spec.variant === 4 && <progress max="90" value={Math.min(90, state.controlValue)}>Live progress</progress>}</header>
        <ol>{model.items.slice(14, 24).map((item, index) => <li key={item.id}><time dateTime={`${item.date}T${9 + index}:00:00Z`}>{9 + index}:00</time><h3>{item.title}</h3><p>{item.summary}</p></li>)}</ol>
      </section>
      <section id="section-3" aria-labelledby="archive-title">
        <h2 id="archive-title">Browse the archive</h2><GalleryTable model={model} state={state} caption="Publication archive" start={12} />
      </section>
      <form id="section-4" data-newsletter="" onSubmit={preventSubmit}>
        <h2>Get the weekly edition</h2><label>Email<input type="email" value={`reader+${state.phase}@example.test`} readOnly /></label>
        <label><input type="checkbox" checked={state.phase !== "mounted"} readOnly />Include the weekend reading list</label>
        <button type="submit">Subscribe</button>
      </form>
    </>
  );
}

function Workspaces({ model, state }: GalleryProps) {
  const cards = galleryOrderedItems(model, state, 0, galleryCardCount(model));
  return (
    <>
      <div className="workspace-shell" id="section-1">
        <aside><h2>Workspace</h2><nav aria-label="Workspace">{model.items.slice(0, 10).map((item, index) => <a key={item.id} href={`#${item.id}`} aria-current={index === 0 ? "page" : undefined}>{item.title}</a>)}</nav><button type="button">Create new</button></aside>
        <section className="workspace-main">
          <header><div><p className="kicker">Current workspace</p><h2>{model.profile.headline}</h2></div><div><button type="button">Share</button><button type="button">More</button></div></header>
          <div className="metrics">{model.items.slice(0, 6).map((item) => <article key={item.id}><span>{item.title}</span><strong>{item.amount}</strong><small>{item.percent}% · {item.status}</small></article>)}</div>
          <div className="board" data-reorder-list="">
            {["Planned", "Active", "Review", "Complete"].map((column, columnIndex) => (
              <section key={column} aria-labelledby={`column-${columnIndex}`}>
                <header><h3 id={`column-${columnIndex}`}>{column}</h3><span>{cards.slice(columnIndex * 5, columnIndex * 5 + 5).length}</span></header>
                {cards.slice(columnIndex * 5, columnIndex * 5 + 5).map((item, index) => <GalleryCard key={item.id} model={model} state={state} item={item} index={columnIndex * 5 + index} action="Inspect" />)}
              </section>
            ))}
          </div>
        </section>
        <aside aria-label="Activity"><h2>Recent activity</h2><ol>{model.items.slice(20, 30).map((item) => <li key={item.id}><strong>{item.owner}</strong><span>{item.title}</span><time dateTime={item.date}>{item.date}</time></li>)}</ol></aside>
      </div>
      <section id="section-2" aria-labelledby="records-title"><h2 id="records-title">Records and assignments</h2><GalleryTable model={model} state={state} caption="Workspace records" start={8} /></section>
      <article className="workspace-editor" aria-labelledby="editor-title">
        <h2 id="editor-title">{model.items[24].title}</h2>
        <div contentEditable suppressContentEditableWarning role="textbox" aria-multiline="true" data-editor="">
          <p>{gallerySentence(model.spec.seed, 34)}</p><h3>Decision record</h3>
          <ul>{model.items.slice(25, 29).map((item) => <li key={item.id}>{item.title}</li>)}</ul>
          <p>{gallerySentence(model.spec.seed + 7, 26)}</p>
        </div>
      </article>
      <dialog open={state.phase !== "mounted"}><h2>Share workspace</h2><p>{model.items[30].summary}</p><button type="button">Copy link</button></dialog>
    </>
  );
}

function Operations({ model, state }: GalleryProps) {
  const bars = model.items.slice(0, 24);
  return (
    <>
      <section className="operations-overview" id="section-1">
        <header><div><p className="kicker">{model.profile.kicker}</p><h2>{model.profile.headline}</h2></div><label>Window<select value={state.phase === "ready" ? "week" : "day"} onChange={() => undefined}><option value="day">Last 24 hours</option><option value="week">Last 7 days</option></select></label></header>
        <div className="metrics">{model.items.slice(0, 8).map((item, index) => <article key={item.id}><span>{item.title}</span><strong data-summary-value={index === 0 ? "" : undefined}>{index === 0 && state.phase === "ready" ? item.amount + model.items[1].percent : item.amount}</strong><small>{item.percent}% · {item.status}</small></article>)}</div>
        {model.spec.variant === 4 && <meter min={0} max={100} low={40} high={85} optimum={90} value={state.controlValue}>Grid health</meter>}
        <svg viewBox="0 0 720 240" role="img" aria-labelledby="operations-chart-title">
          <title id="operations-chart-title">Operational activity by interval</title>
          <g>{bars.map((item, index) => <rect key={item.id} x={index * 29 + 10} y={210 - item.percent * 2} width="18" height={item.percent * 2} data-bar={index} />)}</g>
          <polyline fill="none" stroke="currentColor" points={bars.map((item, index) => `${index * 29 + 19},${210 - ((item.percent + state.controlValue) % 90) * 2}`).join(" ")} />
        </svg>
      </section>
      <div className="operations-grid" id="section-2">
        <section aria-labelledby="signals-title"><h2 id="signals-title">Signals requiring attention</h2><GalleryCards model={model} state={state} action="Review" /></section>
        <aside aria-labelledby="responders-title"><h2 id="responders-title">Owners on rotation</h2>{model.items.slice(12, 20).map((item) => <article key={item.id}><strong>{item.owner}</strong><span>{item.status}</span><small>{item.tags.join(" · ")}</small></article>)}</aside>
      </div>
      <section id="section-3" aria-labelledby="resources-title"><h2 id="resources-title">Resources and controls</h2><GalleryTable model={model} state={state} caption="Operational resources" start={5} /></section>
      <section id="section-4" aria-labelledby="timeline-title"><h2 id="timeline-title">Decision timeline</h2><ol>{model.items.slice(8, 20).map((item, index) => <li key={item.id}><time dateTime={`${item.date}T${8 + index}:00:00Z`}>{8 + index}:00</time><h3>{item.title}</h3><p>{item.summary}</p><span>{item.owner}</span></li>)}</ol></section>
    </>
  );
}

function Community({ model, state }: GalleryProps) {
  return (
    <>
      <section className="community-profile" id="section-1" aria-labelledby="community-title">
        <div role="img" aria-label={`${model.profile.brand} cover`}>{model.profile.brand.slice(0, 2)}</div>
        <div><p className="kicker">{model.profile.kicker}</p><h2 id="community-title">{model.profile.headline}</h2><p>{gallerySentence(model.spec.seed, 30)}</p><button type="button">Join community</button><button type="button">Share</button></div>
        <dl><dt>Members</dt><dd>{model.items[0].amount}</dd><dt>Groups</dt><dd>{model.items[1].percent}</dd><dt>Events</dt><dd>{model.items[2].percent}</dd></dl>
      </section>
      <nav aria-label="Community sections">{["Highlights", "Discussions", "Projects", "Events", "Members"].map((label, index) => <button key={label} type="button" role="tab" aria-selected={(state.phase === "ready" ? 2 : 0) === index}>{label}</button>)}</nav>
      <div className="community-layout" id="section-2">
        <main>
          <form data-composer="" onSubmit={preventSubmit}><label>Start a discussion<textarea rows={3} value={`Share an ${state.phase} update with the community`} readOnly /></label><button type="submit">Publish</button></form>
          <section aria-labelledby="feed-title"><h2 id="feed-title">Community feed</h2><GalleryCards model={model} state={state} action="Respond" /></section>
        </main>
        <aside aria-labelledby="people-title">
          <h2 id="people-title">People to meet</h2>
          {model.items.slice(15, 23).map((item) => <article key={item.id}><span aria-hidden="true">{item.owner.slice(0, 1)}</span><h3>{item.owner}</h3><p>{item.title}</p><button type="button">Connect</button></article>)}
          <fieldset><legend>Weekly poll</legend>{["Morning", "Afternoon", "Evening"].map((label, index) => <label key={label}><input type="radio" name="poll" value={label} checked={(state.phase === "ready" ? 2 : 1) === index} readOnly />{label}</label>)}<button type="button">Vote</button></fieldset>
        </aside>
      </div>
      <section id="section-3" aria-labelledby="events-title"><h2 id="events-title">Upcoming sessions</h2><GalleryTable model={model} state={state} caption="Community schedule" start={6} /></section>
      <section id="section-4" aria-labelledby="guidelines-title"><h2 id="guidelines-title">Guidelines and resources</h2>{model.items.slice(23, 28).map((item, index) => <details key={item.id} open={index === 0 || (index === 1 && state.phase !== "mounted")}><summary>{item.title}</summary><p>{item.summary}</p></details>)}</section>
    </>
  );
}

function ServicePortal({ model, state }: GalleryProps) {
  const extraControl: ReactNode =
    model.spec.variant === 0 ? <><label>Airport<input list="airport-options" value="Shanghai" readOnly /></label><datalist id="airport-options"><option value="Shanghai" /><option value="Tokyo" /><option value="Seoul" /></datalist></> :
    model.spec.variant === 2 ? <label>Transfer allocation<input type="range" min="0" max="100" value={state.controlValue} readOnly /></label> :
    model.spec.variant === 3 ? <label>Claim completion<progress max="100" value={state.controlValue}>{state.controlValue}%</progress></label> :
    model.spec.variant === 4 ? <label>Care plan adherence<meter min={0} max={100} low={40} high={85} optimum={90} value={state.controlValue}>{state.controlValue}%</meter></label> :
    null;
  return (
    <>
      <section id="section-1" className="service-search" aria-labelledby="service-search-title">
        <h2 id="service-search-title">Plan the next step</h2>
        <form data-service-search="" onSubmit={preventSubmit}>
          <div><label>From<input name="from" value={model.spec.variant < 2 ? "Shanghai" : "Primary account"} readOnly /></label><label>To<input name="to" value={state.phase === "ready" ? "Selected service" : "Recommended service"} readOnly /></label></div>
          <div><label>Start date<input type="date" name="start" value="2026-09-18" readOnly /></label><label>End date<input type="date" name="end" value="2026-09-22" readOnly /></label></div>
          <label>People<input type="number" min="1" max="12" value={state.phase === "mounted" ? 2 : 3} readOnly /></label>
          <label>Preference<select value={state.phase === "ready" ? "flexible" : "balanced"} onChange={() => undefined}><optgroup label="Recommended"><option value="balanced">Balanced</option><option value="fastest">Fastest</option></optgroup><optgroup label="Flexible"><option value="flexible">Most flexible</option></optgroup></select></label>
          {extraControl}<button type="submit">Find options</button>
        </form>
      </section>
      <div className="service-layout" id="section-2">
        <section aria-labelledby="options-title"><header><h2 id="options-title">Recommended options</h2><output>{galleryCardCount(model)} available</output></header><GalleryCards model={model} state={state} action="Select" /></section>
        <aside aria-labelledby="summary-title"><h2 id="summary-title">Current selection</h2><ol>{["Profile", "Options", "Details", "Review", "Complete"].map((step, index) => <li key={step} aria-current={index === (state.phase === "ready" ? 2 : 1) ? "step" : undefined}><span>{index + 1}</span>{step}</li>)}</ol><h3>{model.items[0].title}</h3><p>{model.items[0].summary}</p><dl><dt>Estimated value</dt><dd>{galleryMoney(model.items[0].amount)}</dd><dt>Owner</dt><dd>{model.items[0].owner}</dd><dt>Status</dt><dd>{state.phase}</dd></dl><button type="button">Continue</button></aside>
      </div>
      <section id="section-3" aria-labelledby="documents-title"><h2 id="documents-title">Documents and recent activity</h2><GalleryTable model={model} state={state} caption="Service records" start={8} /></section>
      <section id="section-4" aria-labelledby="help-title"><h2 id="help-title">Help for this process</h2>{model.items.slice(18, 24).map((item, index) => <details key={item.id} open={index === 0 || (index === 1 && state.phase !== "mounted")}><summary>{item.title}</summary><p>{item.summary}</p></details>)}</section>
    </>
  );
}

function FamilySurface(props: GalleryProps) {
  switch (props.model.family) {
    case "gallery-marketing": return <Marketing {...props} />;
    case "gallery-commerce": return <Commerce {...props} />;
    case "gallery-editorial": return <Editorial {...props} />;
    case "gallery-workspaces": return <Workspaces {...props} />;
    case "gallery-operations": return <Operations {...props} />;
    case "gallery-community": return <Community {...props} />;
    case "gallery-service-portals": return <ServicePortal {...props} />;
  }
}

function DynamicInfrastructure({ model, state }: GalleryProps) {
  const item = model.items[31];
  return (
    <>
      <section aria-live="polite" aria-atomic="false" data-live-region="">
        <h2>Live page updates</h2>
        <output data-live-status="">{state.phase === "mounted" ? "Waiting for interaction" : `${state.phase} with ${galleryCardCount(model)} interactive records`}</output>
        <div data-dynamic-region="">
          {state.dynamicSequences.map((sequence) => (
            <article key={sequence} data-dynamic-card="" data-dynamic-sequence={sequence}>
              <p className="kicker">Live update</p><h2>{item.title} · {sequence}</h2><p>{item.summary}</p><span>{item.owner}</span>
            </article>
          ))}
        </div>
      </section>
      <template data-update-template="">
        <article data-dynamic-card="" data-dynamic-sequence="template">
          <p className="kicker">Live update</p><h2>{item.title}</h2><p>{item.summary}</p><span>{item.owner}</span>
        </article>
      </template>
      <footer data-gallery-footer="">
        <nav aria-label="Footer"><a href="#gallery-top">Back to top</a><a href="#privacy">Privacy</a><a href="#accessibility">Accessibility</a></nav>
        <p>© 2026 {model.profile.brand}</p>
      </footer>
    </>
  );
}

function GalleryCases({ meta, spec }: ReactCaseProps) {
  const model = useMemo(() => createGalleryViewModel(meta, spec), [meta, spec]);
  const [state, setState] = useState(createGalleryViewState);
  const primaryButton = useRef<HTMLButtonElement>(null);
  const secondaryButton = useRef<HTMLButtonElement>(null);
  const facts = useMemo(() => galleryFacts(model, state), [model, state]);

  function primary(): void {
    setState((current) => galleryPrimaryState(model, current));
  }

  function secondary(): void {
    setState((current) => gallerySecondaryState(model, current));
  }

  useEffect(() => {
    let active = true;
    void (async () => {
      const surface = document.querySelector("[data-gallery-surface]");
      assertFixture(surface, "React gallery surface exists");
      assertFixture(surface.querySelectorAll("*").length >= 120, `${meta.id} has a substantial React tree`);
      assertFixture(surface.querySelectorAll("[data-gallery-card]").length >= 12, `${meta.id} has React-rendered cards`);
      await captureFrame(meta, "mounted");
      if (active) {
        assertFixture(primaryButton.current, "React primary gallery action exists");
        primaryButton.current.click();
      }
    })().catch(failCase);
    return () => {
      active = false;
    };
  }, [meta]);

  useEffect(() => {
    if (state.phase !== "focused") return;
    void (async () => {
      await captureFrame(meta, "interaction-1");
      assertFixture(secondaryButton.current, "React secondary gallery action exists");
      secondaryButton.current.click();
    })().catch(failCase);
  }, [meta, state.phase]);

  useEffect(() => {
    if (state.phase === "ready") markReady(meta, ["mounted", "interaction-1", "react-gallery-ready"]);
  }, [meta, state.phase]);

  return (
    <main id="smoke-root" data-framework="react" data-family={meta.family} data-mode={state.phase}>
      <header data-framework-gallery-header="">
        <div><p>Gallery-inspired complex application</p><h1>{meta.title}</h1></div>
        <div role="group" aria-label="Deterministic gallery transitions">
          <button ref={primaryButton} type="button" data-gallery-primary-action="" aria-pressed={state.phase !== "mounted"} onClick={primary}>Focus and reorder</button>
          <button ref={secondaryButton} type="button" data-gallery-secondary-action="" aria-pressed={state.phase === "ready"} onClick={secondary}>Filter and finalize</button>
        </div>
      </header>
      <section data-case-body="">
        <div data-gallery-surface="" data-gallery-family={model.family} data-gallery-slug={model.spec.slug} data-phase={state.phase} data-ready={state.phase === "ready" ? "true" : undefined}>
          <CommonHeader model={model} state={state} />
          <CommonHero model={model} state={state} />
          <FamilySurface model={model} state={state} />
          <DynamicInfrastructure model={model} state={state} />
        </div>
        <dl data-gallery-facts="">
          {facts.map((fact) => <div key={fact.name} data-fact={fact.name}><dt>{fact.name}</dt><dd>{fact.value}</dd></div>)}
        </dl>
      </section>
    </main>
  );
}

export function mount(meta: ReactCaseProps["meta"], spec: ReactCaseProps["spec"]): void {
  mountReact(GalleryCases, meta, spec);
}

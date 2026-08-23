import { assertFixture } from "./harness";
import type { CaseSpec, SmokeMeta } from "./types";

export type GalleryFamily =
  | "gallery-marketing"
  | "gallery-commerce"
  | "gallery-editorial"
  | "gallery-workspaces"
  | "gallery-operations"
  | "gallery-community"
  | "gallery-service-portals";

export interface GalleryFact {
  name: string;
  value: string;
}

export interface ScenarioProfile {
  brand: string;
  kicker: string;
  headline: string;
}

export interface GalleryItem {
  id: string;
  title: string;
  summary: string;
  owner: string;
  status: string;
  amount: number;
  percent: number;
  date: string;
  tags: [string, string];
}

const GALLERY_FAMILIES = new Set<GalleryFamily>([
  "gallery-marketing",
  "gallery-commerce",
  "gallery-editorial",
  "gallery-workspaces",
  "gallery-operations",
  "gallery-community",
  "gallery-service-portals",
]);

const SCENARIOS: Record<GalleryFamily, readonly ScenarioProfile[]> = {
  "gallery-marketing": [
    {
      brand: "Orbit Agents",
      kicker: "AI operations",
      headline: "Delegate the repetitive work and keep every decision observable",
    },
    {
      brand: "Northstar Climate",
      kicker: "Climate intelligence",
      headline: "Plan resilient infrastructure with evidence from every region",
    },
    {
      brand: "Common Form",
      kicker: "Independent design studio",
      headline: "Identity systems shaped for products that keep changing",
    },
    {
      brand: "Field Assembly",
      kicker: "Architecture practice",
      headline: "Public spaces designed around movement, shade, and memory",
    },
    {
      brand: "Open Hands",
      kicker: "Annual impact report",
      headline: "A year of local programs measured in durable outcomes",
    },
    {
      brand: "Tessera Cloud",
      kicker: "Developer infrastructure",
      headline: "Ship distributed applications without losing the request trail",
    },
    {
      brand: "Redoubt",
      kicker: "Enterprise security",
      headline: "Policy, identity, and incident response in one operating surface",
    },
    {
      brand: "Switchboard",
      kicker: "Product comparison",
      headline: "Compare plans by workflow instead of marketing vocabulary",
    },
    {
      brand: "Forward 2027",
      kicker: "Technology conference",
      headline: "Three days of systems thinking, workshops, and field reports",
    },
    {
      brand: "After Image",
      kicker: "Museum exhibition",
      headline: "An exhibition about archives that change when they are viewed",
    },
  ],
  "gallery-commerce": [
    {
      brand: "Thread Market",
      kicker: "Independent fashion",
      headline: "New-season pieces from small studios and verified resellers",
    },
    {
      brand: "Joinery",
      kicker: "Furniture configurator",
      headline: "Build a modular room from materials you can inspect",
    },
    {
      brand: "Basket Local",
      kicker: "Grocery delivery",
      headline: "A weekly market ordered by harvest window and neighborhood",
    },
    {
      brand: "Signal Supply",
      kicker: "Electronics comparison",
      headline: "Compare repairable devices by components, warranty, and use",
    },
    {
      brand: "Ritual Parcel",
      kicker: "Beauty subscription",
      headline: "A routine assembled from preferences, climate, and cadence",
    },
    {
      brand: "Encore",
      kicker: "Ticket exchange",
      headline: "Verified seats, transparent fees, and live venue inventory",
    },
    {
      brand: "Block Library",
      kicker: "Digital templates",
      headline: "Production-ready systems for documents, sites, and workflows",
    },
    {
      brand: "Table Seven",
      kicker: "Restaurant ordering",
      headline: "Seasonal menus for pickup, delivery, and group ordering",
    },
    {
      brand: "Stockroom",
      kicker: "Wholesale procurement",
      headline: "Compare vendors, lead times, and approval terms in one order",
    },
    {
      brand: "Hammer Live",
      kicker: "Live auction",
      headline: "Timed lots with provenance, bid history, and pickup planning",
    },
  ],
  "gallery-editorial": [
    {
      brand: "The Dispatch",
      kicker: "Live newsroom",
      headline: "Breaking developments organized by evidence and timestamp",
    },
    {
      brand: "Margin",
      kicker: "Culture magazine",
      headline: "Criticism, interviews, and field notes from contemporary work",
    },
    {
      brand: "Open Review",
      kicker: "Research journal",
      headline: "Peer-reviewed studies with methods, datasets, and discussion",
    },
    {
      brand: "Pantry Notes",
      kicker: "Recipe publication",
      headline: "Seasonal recipes with substitutions and coordinated timing",
    },
    {
      brand: "Touchline",
      kicker: "Sports live blog",
      headline: "Minute-by-minute context, formations, and verified statistics",
    },
    {
      brand: "Relay Audio",
      kicker: "Podcast network",
      headline: "Series, transcripts, chapters, and listening queues",
    },
    {
      brand: "Long Exposure",
      kicker: "Photographic essay",
      headline: "A visual field report with captions, maps, and oral histories",
    },
    {
      brand: "Release Notes",
      kicker: "Product changelog",
      headline: "Every shipped change grouped by impact and migration path",
    },
    {
      brand: "Near Here",
      kicker: "City guide",
      headline: "Independent places arranged by neighborhood and time of day",
    },
    {
      brand: "Postmark",
      kicker: "Newsletter archive",
      headline: "Essays, briefings, and reader discussions across every issue",
    },
  ],
  "gallery-workspaces": [
    {
      brand: "Prospect",
      kicker: "Revenue workspace",
      headline: "Accounts, relationships, and next actions without hidden state",
    },
    {
      brand: "Waypoint",
      kicker: "Project command center",
      headline: "Milestones, dependencies, and decisions across active programs",
    },
    {
      brand: "Folio",
      kicker: "Collaborative documents",
      headline: "Structured writing with review threads and publication states",
    },
    {
      brand: "Resolve",
      kicker: "Support inbox",
      headline: "Conversations, service levels, and product context side by side",
    },
    {
      brand: "Tracefield",
      kicker: "Developer observability",
      headline: "Follow requests from deploy through logs, traces, and alerts",
    },
    {
      brand: "Latch",
      kicker: "Feature delivery",
      headline: "Target releases, inspect exposure, and retire stale flags",
    },
    {
      brand: "Palette",
      kicker: "Design token manager",
      headline: "Review semantic tokens across themes, platforms, and releases",
    },
    {
      brand: "Roster",
      kicker: "Applicant tracking",
      headline: "Candidates, interviews, evidence, and decisions in one pipeline",
    },
    {
      brand: "Ledgerstock",
      kicker: "Inventory planning",
      headline: "Demand, replenishment, and warehouse constraints by scenario",
    },
    {
      brand: "Conduit",
      kicker: "Automation builder",
      headline: "Compose triggers, branches, approvals, and recovery paths",
    },
  ],
  "gallery-operations": [
    {
      brand: "Northbound",
      kicker: "Executive analytics",
      headline: "Operating metrics connected to owners, forecasts, and decisions",
    },
    {
      brand: "Incident One",
      kicker: "Incident command",
      headline: "Services, responders, mitigations, and updates on one timeline",
    },
    {
      brand: "Meshview",
      kicker: "Network topology",
      headline: "Inspect routes, dependencies, saturation, and regional health",
    },
    {
      brand: "Freightline",
      kicker: "Logistics control tower",
      headline: "Shipments, exceptions, inventory, and arrivals across the network",
    },
    {
      brand: "Gridroom",
      kicker: "Energy operations",
      headline: "Generation, storage, demand, and constraints by interval",
    },
    {
      brand: "Signal Review",
      kicker: "Fraud operations",
      headline: "Prioritize anomalous activity with evidence and review history",
    },
    {
      brand: "Cohort Lab",
      kicker: "Experiment analysis",
      headline: "Compare variants, segments, guardrails, and decision thresholds",
    },
    {
      brand: "Control Book",
      kicker: "Compliance audit",
      headline: "Controls, evidence, exceptions, and remediation ownership",
    },
    {
      brand: "Spendmap",
      kicker: "Cloud cost explorer",
      headline: "Allocation, commitments, anomalies, and forecasts by service",
    },
    {
      brand: "Queryroom",
      kicker: "Database console",
      headline: "Saved queries, schemas, execution plans, and result history",
    },
  ],
  "gallery-community": [
    {
      brand: "Maker Field",
      kicker: "Creator network",
      headline: "Projects, process notes, collections, and supporter updates",
    },
    {
      brand: "Inside Team",
      kicker: "Company community",
      headline: "Announcements, working groups, questions, and shared rituals",
    },
    {
      brand: "Session Grid",
      kicker: "Conference schedule",
      headline: "Build a personal program across rooms, tracks, and meetups",
    },
    {
      brand: "Common Ground",
      kicker: "Moderation workspace",
      headline: "Reports, context, policy history, and coordinated decisions",
    },
    {
      brand: "Studio Cohort",
      kicker: "Learning community",
      headline: "Lessons, critiques, office hours, and peer milestones",
    },
    {
      brand: "Good Turn",
      kicker: "Volunteer network",
      headline: "Local needs matched with shifts, skills, and team capacity",
    },
    {
      brand: "Block Party",
      kicker: "Neighborhood forum",
      headline: "Questions, notices, events, and resources around the corner",
    },
    {
      brand: "Raid Table",
      kicker: "Gaming guild",
      headline: "Rosters, encounters, loadouts, and availability by team",
    },
    {
      brand: "Practice Index",
      kicker: "Professional directory",
      headline: "Find peers by discipline, location, availability, and work",
    },
    {
      brand: "Backchannel",
      kicker: "Live stream community",
      headline: "Chapters, reactions, questions, and moderation in real time",
    },
  ],
  "gallery-service-portals": [
    {
      brand: "Vector Air",
      kicker: "Flight search",
      headline: "Compare flexible journeys by schedule, stops, and fare terms",
    },
    {
      brand: "Still House",
      kicker: "Hotel booking",
      headline: "Rooms, amenities, policies, and local recommendations",
    },
    {
      brand: "Harbor Bank",
      kicker: "Digital banking",
      headline: "Balances, transfers, recurring payments, and cash flow",
    },
    {
      brand: "Coverline",
      kicker: "Insurance claims",
      headline: "Report an incident, attach evidence, and follow every decision",
    },
    {
      brand: "Care Path",
      kicker: "Patient portal",
      headline: "Appointments, care plans, messages, and test history",
    },
    {
      brand: "Coursework",
      kicker: "Enrollment portal",
      headline: "Programs, prerequisites, schedules, and degree progress",
    },
    {
      brand: "Open Door",
      kicker: "Property search",
      headline: "Compare homes by neighborhood, history, and monthly cost",
    },
    {
      brand: "Current Home",
      kicker: "Energy account",
      headline: "Usage, tariffs, forecasts, and efficiency actions",
    },
    {
      brand: "Civic Desk",
      kicker: "Public benefits",
      headline: "Eligibility, applications, documents, and case messages",
    },
    {
      brand: "People Ledger",
      kicker: "Payroll and benefits",
      headline: "Pay, elections, time off, documents, and life events",
    },
  ],
};

const FAMILY_TERMS: Record<GalleryFamily, readonly string[]> = {
  "gallery-marketing": [
    "Adaptive platform",
    "Customer evidence",
    "Regional program",
    "Workflow library",
    "Field research",
    "Partner network",
    "Implementation guide",
    "Impact model",
    "Migration path",
    "Operating principle",
    "Case study",
    "Launch briefing",
  ],
  "gallery-commerce": [
    "Essential collection",
    "Limited release",
    "Verified edition",
    "Studio series",
    "Seasonal selection",
    "Member favorite",
    "Restored original",
    "Bundle offer",
    "Professional kit",
    "Archive find",
    "Custom configuration",
    "Local delivery",
  ],
  "gallery-editorial": [
    "Field report",
    "Editor briefing",
    "Reader dispatch",
    "Methods note",
    "Long-form interview",
    "Visual investigation",
    "Live update",
    "Archive edition",
    "Critical review",
    "Data notebook",
    "Audio transcript",
    "Community letter",
  ],
  "gallery-workspaces": [
    "Launch program",
    "Renewal account",
    "Migration project",
    "Escalation thread",
    "Release workspace",
    "Review queue",
    "Automation draft",
    "Design system",
    "Hiring plan",
    "Inventory cycle",
    "Customer request",
    "Quarterly objective",
  ],
  "gallery-operations": [
    "Regional cluster",
    "Priority incident",
    "Capacity forecast",
    "Exception queue",
    "Control review",
    "Service allocation",
    "Experiment cohort",
    "Cost anomaly",
    "Network route",
    "Demand interval",
    "Audit sample",
    "Query workload",
  ],
  "gallery-community": [
    "Member project",
    "Working group",
    "Upcoming session",
    "Open question",
    "Shared resource",
    "Community notice",
    "Review request",
    "Volunteer shift",
    "Learning milestone",
    "Moderation report",
    "Live discussion",
    "Local event",
  ],
  "gallery-service-portals": [
    "Recommended option",
    "Current application",
    "Upcoming appointment",
    "Saved comparison",
    "Required document",
    "Recent activity",
    "Payment schedule",
    "Service message",
    "Eligibility step",
    "Account preference",
    "Booking selection",
    "Plan summary",
  ],
};

const OWNERS = [
  "Ada Chen",
  "Lin Park",
  "Mira Singh",
  "Noah Williams",
  "Omar Haddad",
  "Rin Sato",
  "Sora Kim",
  "Tao Rivera",
  "Uma Patel",
  "Vera Novak",
];

const STATUSES = ["draft", "active", "review", "scheduled", "resolved", "paused"];
const COPY_WORDS = [
  "clear",
  "measurable",
  "connected",
  "durable",
  "local",
  "shared",
  "verified",
  "adaptive",
  "focused",
  "open",
  "practical",
  "timely",
];

function isGalleryFamily(value: string): value is GalleryFamily {
  return GALLERY_FAMILIES.has(value as GalleryFamily);
}

function sentence(seed: number, count: number): string {
  return Array.from(
    { length: count },
    (_, index) => COPY_WORDS[(seed + index * 5) % COPY_WORDS.length],
  ).join(" ");
}

function money(cents: number): string {
  return `$${Math.floor(cents / 100)}.${String(cents % 100).padStart(2, "0")}`;
}

function scenarioFor(family: GalleryFamily, variant: number): ScenarioProfile {
  const profile = SCENARIOS[family][variant];
  assertFixture(profile, `gallery scenario ${family}/${variant} exists`);
  return profile;
}

function galleryItems(family: GalleryFamily, spec: CaseSpec, count: number): GalleryItem[] {
  const terms = FAMILY_TERMS[family];
  return Array.from({ length: count }, (_, index) => {
    const value = spec.seed + index * 17 + spec.variant * 11;
    return {
      id: `${spec.slug}-item-${index + 1}`,
      title: `${terms[(value + index) % terms.length]} ${index + 1}`,
      summary: sentence(value, 11 + (index % 5)),
      owner: OWNERS[(value * 3 + index) % OWNERS.length],
      status: STATUSES[(value + index * 2) % STATUSES.length],
      amount: 900 + ((value * 137 + index * 311) % 48_000),
      percent: 12 + ((value * 7 + index * 13) % 86),
      date: `2026-${String(8 + (index % 4)).padStart(2, "0")}-${String(
        1 + ((value + index) % 27),
      ).padStart(2, "0")}`,
      tags: [
        terms[(value + 3) % terms.length].split(" ")[0].toLowerCase(),
        STATUSES[(value + 2) % STATUSES.length],
      ],
    };
  });
}

export type GalleryPhase = "mounted" | "focused" | "ready";

export interface GalleryViewModel {
  family: GalleryFamily;
  meta: SmokeMeta;
  spec: CaseSpec;
  profile: ScenarioProfile;
  items: GalleryItem[];
  navLabels: readonly string[];
}

export interface GalleryViewState {
  phase: GalleryPhase;
  selectedId: string | null;
  query: string;
  dynamicSequences: readonly string[];
  controlValue: number;
}

const GALLERY_NAV_LABELS: Record<GalleryFamily, readonly string[]> = {
  "gallery-marketing": ["Overview", "Stories", "Capabilities", "Plans", "About"],
  "gallery-commerce": ["New", "Collections", "Brands", "Offers", "Orders"],
  "gallery-editorial": ["Latest", "Features", "Opinion", "Audio", "Archive"],
  "gallery-workspaces": ["Home", "Work", "Reports", "Automations", "Team"],
  "gallery-operations": ["Overview", "Signals", "Resources", "Incidents", "Audit"],
  "gallery-community": ["Feed", "Groups", "Events", "Members", "Messages"],
  "gallery-service-portals": ["Summary", "Explore", "Applications", "Documents", "Support"],
};

const GALLERY_CARD_COUNTS: Record<GalleryFamily, number> = {
  "gallery-marketing": 16,
  "gallery-commerce": 18,
  "gallery-editorial": 14,
  "gallery-workspaces": 20,
  "gallery-operations": 12,
  "gallery-community": 15,
  "gallery-service-portals": 14,
};

const GALLERY_TABLE_COUNTS: Record<GalleryFamily, number> = {
  "gallery-marketing": 0,
  "gallery-commerce": 10,
  "gallery-editorial": 12,
  "gallery-workspaces": 14,
  "gallery-operations": 16,
  "gallery-community": 10,
  "gallery-service-portals": 12,
};

export function createGalleryViewModel(meta: SmokeMeta, spec: CaseSpec): GalleryViewModel {
  assertFixture(isGalleryFamily(meta.family), `known gallery family: ${meta.family}`);
  const family = meta.family;
  return {
    family,
    meta,
    spec,
    profile: scenarioFor(family, spec.variant),
    items: galleryItems(family, spec, 32),
    navLabels: GALLERY_NAV_LABELS[family],
  };
}

export function createGalleryViewState(): GalleryViewState {
  return {
    phase: "mounted",
    selectedId: null,
    query: "all",
    dynamicSequences: [],
    controlValue: 42,
  };
}

export function galleryPrimaryState(
  model: GalleryViewModel,
  current: GalleryViewState,
): GalleryViewState {
  assertFixture(
    current.phase === "mounted",
    `${model.meta.id} primary transition starts from mounted`,
  );
  const selectedIndex = (model.spec.seed + model.spec.variant * 3) % GALLERY_CARD_COUNTS[model.family];
  return {
    phase: "focused",
    selectedId: model.items[selectedIndex].id,
    query: model.profile.kicker,
    dynamicSequences: ["focused"],
    controlValue: 53,
  };
}

export function gallerySecondaryState(
  model: GalleryViewModel,
  current: GalleryViewState,
): GalleryViewState {
  assertFixture(
    current.phase === "focused",
    `${model.meta.id} secondary transition starts from focused`,
  );
  return {
    ...current,
    phase: "ready",
    query: `${model.profile.kicker} ready`,
    dynamicSequences: ["focused", "ready"],
    controlValue: 72,
  };
}

export function galleryOrderedItems(
  model: GalleryViewModel,
  state: GalleryViewState,
  start = 0,
  count = model.items.length - start,
): GalleryItem[] {
  const selected = model.items.slice(start, start + count);
  if (state.phase === "mounted" || selected.length < 2) {
    return selected;
  }
  if (state.phase === "focused") {
    return [selected[selected.length - 1], ...selected.slice(0, -1)];
  }
  return [selected[selected.length - 1], selected[0], ...selected.slice(1, -1)];
}

export function galleryCardHidden(
  model: GalleryViewModel,
  state: GalleryViewState,
  item: GalleryItem,
  index: number,
): boolean {
  return state.phase === "ready" && index % 4 === 1 && item.id !== state.selectedId;
}

export function galleryCardCount(model: GalleryViewModel): number {
  return GALLERY_CARD_COUNTS[model.family];
}

export function galleryTableCount(model: GalleryViewModel): number {
  return GALLERY_TABLE_COUNTS[model.family];
}

export function galleryFacts(
  model: GalleryViewModel,
  state: GalleryViewState,
): GalleryFact[] {
  const cardCount = galleryCardCount(model);
  const orderedCards = galleryOrderedItems(model, state, 0, cardCount);
  const hiddenCards =
    state.phase === "ready"
      ? orderedCards.filter((item, index) => galleryCardHidden(model, state, item, index)).length
      : 0;
  const detailsCount =
    model.family === "gallery-marketing" ||
    model.family === "gallery-community" ||
    model.family === "gallery-service-portals"
      ? state.phase === "mounted"
        ? 1
        : 2
      : 0;
  return [
    { name: "phase", value: state.phase },
    { name: "selected", value: state.selectedId ?? "none" },
    { name: "visible-cards", value: String(cardCount - hiddenCards) },
    { name: "table-rows", value: String(galleryTableCount(model)) },
    { name: "dynamic-cards", value: String(state.dynamicSequences.length) },
    { name: "open-details", value: String(detailsCount) },
    { name: "query", value: state.query },
  ];
}

export function galleryMoney(cents: number): string {
  return money(cents);
}

export function gallerySentence(seed: number, count: number): string {
  return sentence(seed, count);
}

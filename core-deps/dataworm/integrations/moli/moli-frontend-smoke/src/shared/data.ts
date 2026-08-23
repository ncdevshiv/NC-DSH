export interface StableItem {
  id: string;
  title: string;
  owner: string;
  status: "new" | "active" | "paused" | "done";
  amount: number;
  tags: string[];
}

const TITLES = [
  "Parser compatibility",
  "Network scheduler",
  "DOM snapshot",
  "Cookie partition",
  "Runtime lifecycle",
  "Selector engine",
  "Storage quota",
  "Module graph",
  "Worker channel",
  "Protocol routing",
  "Style invalidation",
  "Fetch priority",
];

const OWNERS = ["Ada", "Lin", "Mira", "Noah", "Omar", "Rin", "Sora", "Tao"];
const STATUSES: StableItem["status"][] = ["new", "active", "paused", "done"];
const TAGS = ["dom", "network", "runtime", "cdp", "css", "storage", "worker", "forms"];

export function stableItems(seed: number, count: number): StableItem[] {
  return Array.from({ length: count }, (_, index) => {
    const value = seed + index * 13;
    return {
      id: `item-${seed}-${index}`,
      title: TITLES[value % TITLES.length],
      owner: OWNERS[(value * 3) % OWNERS.length],
      status: STATUSES[(value + index) % STATUSES.length],
      amount: 125 + ((value * 97) % 9900),
      tags: [TAGS[value % TAGS.length], TAGS[(value + 3) % TAGS.length]],
    };
  });
}

export function money(cents: number): string {
  const whole = Math.floor(cents / 100);
  const remainder = String(cents % 100).padStart(2, "0");
  return `$${whole}.${remainder}`;
}

export function deterministicWords(seed: number, count: number): string {
  const words = [
    "lightweight",
    "observable",
    "deterministic",
    "compatible",
    "structured",
    "isolated",
    "semantic",
    "diagnostic",
    "reactive",
    "incremental",
    "portable",
    "stable",
  ];
  return Array.from({ length: count }, (_, index) => words[(seed + index * 5) % words.length]).join(
    " ",
  );
}

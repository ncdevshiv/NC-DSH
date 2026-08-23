// Hand-rolled force-directed graph model (~150 lines): no deps.
// Physics: uniform-grid pairwise repulsion + typed-edge springs + contains-tree
// gravity hints + global centering, integrated with velocity damping.
// The canvas component (GraphCanvas.tsx) owns rendering + interaction.

export type EdgeType = "contains" | "references" | "duplicate_of" | "similar_to";

export interface GNode {
  id: string;
  kind: string; // "dir" | "file"
  x: number;
  y: number;
  vx: number;
  vy: number;
}

export interface GEdge {
  src: string;
  dst: string;
  type: EdgeType;
}

// Rest lengths per edge dimension (tuned like the legacy page).
const REST: Record<EdgeType, number> = {
  contains: 70,
  references: 120,
  duplicate_of: 90,
  similar_to: 110,
};

const RENDER_CAP = 800;
const CELL = 90; // spatial-hash cell size for repulsion lookups

function hashKey(type: string, src: string, dst: string): string {
  return `${type}\u0000${src}\u0000${dst}`;
}

let spiralN = 0;
function spawnPos(parent?: GNode): { x: number; y: number } {
  if (parent) {
    const a = Math.random() * Math.PI * 2;
    return { x: parent.x + Math.cos(a) * REST.contains, y: parent.y + Math.sin(a) * REST.contains };
  }
  const a = spiralN * 2.39996; // golden-angle spiral => even spread
  const r = 14 * Math.sqrt(++spiralN);
  return { x: Math.cos(a) * r, y: Math.sin(a) * r };
}

export class GraphModel {
  nodes = new Map<string, GNode>();
  edges = new Map<string, GEdge>();
  childrenOf = new Map<string, string[]>();
  /** ids currently simulated/rendered, insertion order (FIFO beyond cap). */
  order: string[] = [];
  rendered = new Set<string>();
  /** every node ever ingested — drives the "showing N of M" note */
  totalSeen = 0;

  version = 0; // bumped on structural change so UI re-reads counts

  /** kind optional: omitted (edge ingestion) defaults on create, never overwrites. */
  private ensure(id: string, kind?: string): GNode {
    let n = this.nodes.get(id);
    if (!n) {
      // Fragment roots ("#root:<abs-path>") are top-level: never derive a
      // parent from the slashes inside their embedded path.
      const cut = id.startsWith("#root:") ? -1 : Math.max(id.lastIndexOf("/"), id.lastIndexOf("\\"));
      const parentId = cut > 0 ? id.slice(0, cut) : "";
      const p = this.nodes.get(parentId);
      const pos = spawnPos(p);
      n = { id, kind: kind || "file", x: pos.x, y: pos.y, vx: 0, vy: 0 };
      this.nodes.set(id, n);
    } else if (kind && n.kind !== kind) {
      n.kind = kind;
    }
    this.admit(id);
    return n;
  }

  /** Mark a node as rendered, evicting oldest unprotected nodes over the cap. */
  private admit(id: string) {
    if (this.rendered.has(id)) return;
    this.order.push(id);
    this.rendered.add(id);
    while (this.order.length > RENDER_CAP) {
      const victim = this.order[0];
      this.order.shift();
      this.rendered.delete(victim);
    }
    this.version++;
  }

  ingestNode(id: string, kind: string) {
    if (!this.nodes.has(id)) this.totalSeen++; // unique nodes, not events
    this.ensure(id, kind);
  }

  ingestBatch(batch: { id: string; node_kind?: string }[]) {
    for (const b of batch) this.ingestNode(String(b.id), String(b.node_kind ?? "file"));
  }

  ingestEdge(src: string, dst: string, type: EdgeType) {
    const key = hashKey(type, src, dst);
    if (this.edges.has(key)) return;
    // Edge ingestion asserts no kind — node events carry the authoritative kind.
    const s = this.ensure(src);
    const d = this.ensure(dst);
    void s;
    void d;
    this.edges.set(key, { src, dst, type });
    if (type === "contains") {
      (this.childrenOf.get(src) ?? this.childrenOf.set(src, []).get(src)!).push(dst);
    }
    this.version++;
  }

  resetDim(type: EdgeType) {
    let changed = false;
    for (const [k, e] of this.edges) {
      if (e.type === type) {
        this.edges.delete(k);
        changed = true;
      }
    }
    if (type === "contains") this.childrenOf.clear();
    if (changed) this.version++;
  }

  /** Search-focus: force specific ids (target + neighbours) into the render set. */
  focusIds(ids: string[]): string[] {
    const admitted: string[] = [];
    for (const id of ids) {
      if (!this.nodes.has(id)) continue;
      if (!this.rendered.has(id)) {
        this.admit(id);
        admitted.push(id);
      }
    }
    return admitted;
  }

  get renderedCount(): number {
    return this.rendered.size;
  }

  /** {shown, total} for the "showing N of M" note. */
  get counts(): { shown: number; total: number } {
    return { shown: this.rendered.size, total: this.totalSeen };
  }

  // ---- physics -------------------------------------------------------------

  tick() {
    const ns: GNode[] = [];
    for (const id of this.order) {
      const n = this.nodes.get(id);
      if (n) ns.push(n);
    }
    if (!ns.length) return;

    // Uniform spatial hash over current positions.
    const grid = new Map<string, GNode[]>();
    for (const n of ns) {
      const k = `${Math.floor(n.x / CELL)},${Math.floor(n.y / CELL)}`;
      (grid.get(k) ?? grid.set(k, []).get(k)!).push(n);
    }

    // Pairwise repulsion (neighbour cells only — keeps it ~O(n·k)).
    const REPULSE = 2600;
    for (const n of ns) {
      const cx = Math.floor(n.x / CELL);
      const cy = Math.floor(n.y / CELL);
      for (let gx = cx - 1; gx <= cx + 1; gx++) {
        for (let gy = cy - 1; gy <= cy + 1; gy++) {
          const cell = grid.get(`${gx},${gy}`);
          if (!cell) continue;
          for (const m of cell) {
            if (m.id <= n.id) continue;
            let dx = n.x - m.x;
            let dy = n.y - m.y;
            let d2 = dx * dx + dy * dy;
            if (d2 > CELL * CELL) continue;
            if (d2 < 1) {
              dx = (Math.random() - 0.5) * 2;
              dy = (Math.random() - 0.5) * 2;
              d2 = 4;
            }
            const f = REPULSE / d2;
            const d = Math.sqrt(d2);
            const fx = (dx / d) * f;
            const fy = (dy / d) * f;
            n.vx += fx;
            n.vy += fy;
            m.vx -= fx;
            m.vy -= fy;
          }
        }
      }
    }

    // Typed springs + contains-tree gravity (children drift toward parents).
    for (const e of this.edges.values()) {
      const a = this.nodes.get(e.src);
      const b = this.nodes.get(e.dst);
      if (!a || !b || !this.rendered.has(e.src) || !this.rendered.has(e.dst)) continue;
      const dx = b.x - a.x;
      const dy = b.y - a.y;
      const d = Math.max(Math.hypot(dx, dy), 1);
      const rest = REST[e.type];
      const f = (d - rest) * 0.015;
      const ux = (dx / d) * f;
      const uy = (dy / d) * f;
      a.vx += ux;
      a.vy += uy;
      b.vx -= ux;
      b.vy -= uy;
      if (e.type === "contains") {
        b.vx += (a.x + (dx >= 0 ? 40 : -40) - b.x) * 0.004;
        b.vy += (a.y - b.y) * 0.004;
      }
    }

    // Integrate with damping + gentle global centering.
    let moving = 0;
    for (const n of ns) {
      n.vx -= n.x * 0.0015;
      n.vy -= n.y * 0.0015;
      n.vx *= 0.85;
      n.vy *= 0.85;
      const sp = Math.hypot(n.vx, n.vy);
      if (sp > 30) {
        n.vx = (n.vx / sp) * 30;
        n.vy = (n.vy / sp) * 30;
      }
      n.x += n.vx;
      n.y += n.vy;
      if (sp > 0.15) moving++;
    }
    return moving;
  }
}

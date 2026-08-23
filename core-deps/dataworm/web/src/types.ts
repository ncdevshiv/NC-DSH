// Shared API types for the DataWorm daemon surface (see dataworm/core.py).

export interface Summary {
  root?: string;
  meta?: Record<string, unknown>;
  node_kinds?: Record<string, number>;
  nodes?: number;
  edges?: number;
  edges_contains?: number;
  edges_references?: number;
  edges_duplicate_of?: number;
  edges_similar_to?: number;
  /** federation-wide across all fragments (excluding external/shadow nodes); absent on old daemons */
  total?: { nodes: number; edges: number; [k: string]: unknown };
  /** count of external shadow nodes */
  shadows?: number;
  /** per-fragment counts, sorted by root */
  fragments?: { root: string; nodes: number; edges: number }[];
  [k: string]: unknown;
}

/** Cold-start graph snapshot from GET /api/graph (see Core._op_graph). */
export interface GraphSnapshot {
  root?: string;
  fragments?: string[];
  nodes?: { id: string; node_kind?: string }[];
  edges?: { src: string; dst: string; edge_type: string }[];
  edges_truncated?: number;
}

export interface Ping {
  ok?: boolean;
  backend?: string;
  db?: string;
}

export interface SearchHit {
  id: string;
  kind: string;
  path: string;
}

export interface ContextLink {
  id: string;
  type: string;
  weight: number;
  direction: "in" | "out";
  cross_dir?: boolean;
  dir?: string;
}

export interface ContextResult {
  node?: { id: string; path: string; kind: string; size?: number; mtime?: number };
  link_counts?: Record<string, number>;
  links?: ContextLink[];
  dangling_references?: string[];
  impact?: ImpactResult | { error: string };
  error?: string;
}

export interface ImpactResult {
  target?: string;
  direct?: string[];
  transitive?: string[];
  total_affected?: number;
  truncated?: boolean;
  error?: string;
}

export interface PlanEditResult {
  path?: string;
  unchanged?: boolean;
  refs_gained?: string[];
  refs_lost?: string[];
  dangling_now?: string[];
  exact_duplicate_of?: string;
  near_duplicates?: { id: string; hamming: number }[];
  dependents_count?: number;
  error?: string;
}

/** Change report riding under event.report for SSE kind=="change" events. */
export interface ChangeReport {
  ts?: number;
  kind: "created" | "modified" | "deleted" | "moved" | "burst";
  path: string;
  root?: string;
  old_hash?: string;
  new_hash?: string;
  refs_lost?: string[];
  refs_gained?: string[];
  dangling_now?: string[];
  dependents_before?: string[];
  dependents_after?: string[];
  paths?: string[];
  source?: string;
  seq?: number;
}

export type BusEvent = {
  seq: number;
  kind: string;
  [k: string]: unknown;
};

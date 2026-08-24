/**
 * Wire types for SearXNG's JSON search API (`GET <base>/search?format=json`).
 * Types only — no runtime code. A SearXNG instance aggregates upstream engines:
 * each entry of `results[]` carries a URL with optional title, text excerpt,
 * publication date, and the engine that produced it; instant answers arrive as
 * `answers[]` and structured sidebar entities as `infoboxes[]`.
 *
 * @module @deepseek-ai/dsh-web-search-searxng/types
 */

/** One entry of SearXNG's aggregated `results[]`. */
export interface SearxngResult {
  /**
   * Result URL. Aggregation can emit entries without one (e.g. some engine
   * answers); they carry nothing citeable and are skipped by the mapper.
   */
  url?: string | null
  title?: string | null
  /** Text excerpt from the upstream engine or the page's meta description. */
  content?: string | null
  /** Publication/crawl date as an ISO-8601 string, when an engine supplies one. */
  publishedDate?: string | null
  /** Upstream engine that produced the entry (e.g. `duckduckgo`, `wikipedia`). */
  engine?: string | null
}

/** SearXNG's search response envelope. */
export interface SearxngSearchResponse {
  /** The query as SearXNG parsed it. */
  query?: string
  results?: SearxngResult[]
  /** Instant-answer strings (calculator, special queries); usually empty for web searches. */
  answers?: string[]
  /** Structured infobox entities; no portable mapping exists on the seam. */
  infoboxes?: unknown[]
}

/** SearXNG's error response envelope (best-effort; fields vary by failure). */
export interface SearxngError {
  error?: string
  message?: string
}

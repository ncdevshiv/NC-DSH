/**
 * Model-capability inference. Model lists (the OpenAI `/models` listing, the
 * installed pi-ai catalog) carry no capability metadata — only id, name, and
 * capacity — so a newly-added multimodal model defaults to text-only unless
 * declared by hand. This module holds the curated knowledge base that bridges
 * that gap: when a row's id or name matches a well-known multimodal family, it
 * seeds the modality list with `['text','image']`. Unrecognized models return
 * `undefined`, which leaves the caller's text-only default untouched (fail
 * closed rather than guessing a capability the endpoint may reject).
 *
 * The table is id-/name-prefix based on purpose: model ids carry version
 * suffixes (`-20250512`, `.2`, `-preview`) and provider routing labels that a
 * full-id equality test would never match. A prefix that is unambiguous within
 * its provider is enough. Cases are folded because different gateways spell the
 * same model differently (`Gemini-1.5-Pro` vs `gemini-1.5-pro`).
 *
 * Extending: add one line per family to {@link MULTIMODAL_PREFIXES}, most
 * specific prefix first. When a family becomes text-only (or adds a new
 * capability) it stays listed here rather than removed, so the entry can be
 * narrowed in place.
 *
 * @module dsh-llm/capabilities
 */

import type { ModelModality } from './types.ts'

/** A curated prefix that identifies one known multimodal model family. */
interface MultimodalFamily {
  /** Id or name prefix that gates this family; matched case-insensitively. */
  prefix: string
  /** Accepted input modalities; `text` is implicit, `image` is what matters. */
  modalities: readonly ModelModality[]
}

/**
 * Every model family this build knows carries image input. Ordered most
 * specific prefix first so `gpt-4o` is not swallowed by a broader `gpt-` match.
 * Every prefix is a lower-case fragment of the model id or display name.
 *
 * Keep entries conservative: a false positive silently persists image input
 * the endpoint rejects on every turn; a false negative just requires a manual
 * checkbox. Prefer the false-negative when a family's multimodal status is
 * uncertain.
 */
const MULTIMODAL_PREFIXES: ReadonlyArray<MultimodalFamily> = [
  // OpenAI: GPT-4 and every later generation accept images; o-series
  // reasoning models also accept images.
  { prefix: 'gpt-4o', modalities: ['text', 'image'] },
  { prefix: 'gpt-4.5', modalities: ['text', 'image'] },
  { prefix: 'gpt-4.1', modalities: ['text', 'image'] },
  { prefix: 'gpt-4', modalities: ['text', 'image'] },
  { prefix: 'gpt-5', modalities: ['text', 'image'] },
  { prefix: 'o3-', modalities: ['text', 'image'] },
  { prefix: 'o4-', modalities: ['text', 'image'] },
  // Anthropic: every Claude 3 and later model is multimodal.
  { prefix: 'claude-', modalities: ['text', 'image'] },
  // Google: every Gemini model is multimodal.
  { prefix: 'gemini-', modalities: ['text', 'image'] },
  // DeepSeek: V3 and the V3.2+ generation are multimodal.
  { prefix: 'deepseek-v3', modalities: ['text', 'image'] },
  // Qwen: the VL (vision-language) variants and the multimodal Qwen3 line.
  { prefix: 'qwen-vl', modalities: ['text', 'image'] },
  { prefix: 'qwen2-vl', modalities: ['text', 'image'] },
  { prefix: 'qwen2.5-vl', modalities: ['text', 'image'] },
  { prefix: 'qwen3', modalities: ['text', 'image'] },
  // Zhipu GLM: vision variants and the multimodal-plus/flash line.
  { prefix: 'glm-4v', modalities: ['text', 'image'] },
  { prefix: 'glm-4.5v', modalities: ['text', 'image'] },
  { prefix: 'glm-4-plus', modalities: ['text', 'image'] },
  { prefix: 'glm-4-flash', modalities: ['text', 'image'] },
  { prefix: 'glm-4', modalities: ['text', 'image'] },
  // Moonshot Kimi: the k-series is multimodal.
  { prefix: 'kimi-', modalities: ['text', 'image'] },
  // ByteDance Doubao: the 1.6+ multimodal line and the explicit vision line.
  { prefix: 'doubao-1.6', modalities: ['text', 'image'] },
  { prefix: 'doubao-1.5-vision', modalities: ['text', 'image'] },
  { prefix: 'doubao-', modalities: ['text', 'image'] },
  // MiniMax: m2.6+ multimodal.
  { prefix: 'minimax-', modalities: ['text', 'image'] },
  // Tencent Hunyuan: multimodal.
  { prefix: 'hunyuan-', modalities: ['text', 'image'] },
  // 01AI: the Yi VL line is multimodal.
  { prefix: 'yi-vl', modalities: ['text', 'image'] },
  // StepFun: the Step-2 multimodal line.
  { prefix: 'step-', modalities: ['text', 'image'] },
]

/** Fold a candidate id or name for case-insensitive prefix matching. */
function normalize(value: string): string {
  return value.trim().toLowerCase()
}

/**
 * Infer the accepted input modalities for one model from its id and display
 * name. This is a knowledge-lookup, not a network call: the OpenAI `/models`
 * listing and the installed pi-ai catalog carry no capability fields, so a
 * newly-added model defaults to text-only until declared.
 *
 * @param id - the model id (e.g. `gemini-1.5-pro`, `deepseek-v3`).
 * @param name - the display name, consulted when the id is bare (e.g.
 *   `gemini`); defaults to `id` when `undefined`.
 * @returns the inferred modalities for known multimodal families, or
 *   `undefined` when the model is unrecognized — the caller falls back to its
 *   text-only default.
 */
export function inferModelModalities(
  id: string,
  name?: string,
): readonly ModelModality[] | undefined {
  const candidates = [normalize(id), ...(name !== undefined ? [normalize(name)] : [])]
  for (const family of MULTIMODAL_PREFIXES) {
    if (candidates.some(candidate => candidate.startsWith(family.prefix))) {
      return family.modalities
    }
  }
  return undefined
}

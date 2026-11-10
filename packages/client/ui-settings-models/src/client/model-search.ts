/**
 * Matching for the model search field: the text a user types narrows one
 * provider's model list while it is typed.
 *
 * A term matches a model by its id or its display name, ignoring case and the
 * separators inside them, so `gpt4o` finds `gpt-4o` and `4o mini` finds
 * `gpt-4o-mini`. Several terms each have to match, so `flash v4` finds
 * `deepseek-v4-flash`. A term of three characters or more also matches when
 * its characters appear in that order inside one word, so `gmn` finds
 * `gemini`.
 */

/** The text one search term is compared against. */
export interface ModelSearchTarget {
  /** The model's id. */
  readonly id: string
  /** The model's display name, when it has one. */
  readonly name?: string | undefined
}

/** A term shorter than this is too easy to satisfy by accident. */
const FUZZY_MIN_LENGTH = 3

/**
 * Lowercase the text and collapse every run of separators into one space, so
 * ids, display names, and typed terms compare on the same footing.
 * @param value - the text to normalize.
 * @returns the normalized text.
 */
function normalize(value: string): string {
  return value.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, ' ').trim()
}

/**
 * Whether the characters of `needle` appear in `haystack` in that order, gaps
 * allowed.
 * @param needle - the characters to look for; never empty.
 * @param haystack - the text to look in.
 * @returns whether every character was found in order.
 */
function isSubsequence(needle: string, haystack: string): boolean {
  const characters = Array.from(needle)
  let at = 0
  for (const character of haystack) {
    if (character === characters[at]) at += 1
    if (at === characters.length) return true
  }
  return false
}

/**
 * Whether one typed term names a model.
 * @param words - the model's id and display name, normalized.
 * @param packed - the same text without its separators.
 * @param term - the typed term, normalized.
 * @returns whether the term names the model.
 */
function matchesTerm(words: string, packed: string, term: string): boolean {
  if (words.includes(term)) return true
  if (packed.includes(term)) return true
  if (Array.from(term).length < FUZZY_MIN_LENGTH) return false
  // The loose match stays inside one word: the characters of a term spread
  // across several words would name models the user never asked for.
  return words.split(' ').some(word => isSubsequence(term, word))
}

/**
 * Whether one model matches a typed search term.
 * @param target - the model's id and display name.
 * @param query - the search text as typed.
 * @returns whether the model belongs in the filtered list.
 */
export function modelMatches(target: ModelSearchTarget, query: string): boolean {
  const terms = normalize(query).split(' ').filter(term => term.length > 0)
  if (terms.length === 0) return true
  const words = normalize(`${target.id} ${target.name ?? ''}`)
  const packed = words.replace(/ /g, '')
  return terms.every(term => matchesTerm(words, packed, term))
}

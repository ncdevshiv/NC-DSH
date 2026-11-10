/**
 * Fixed synthetic corpus shared by the workspace retrieval benchmark worker
 * and its benchmark file. Every document, vector, and query is generated from
 * reviewed constants; no user data, repository content, recorded Session, or
 * network service is read.
 */

/** Documents in the measured corpus. */
export const CORPUS_DOCUMENTS = 2_000

/** Category facets used by synthetic semantic vectors. */
const CATEGORIES = 8

/** Dense vector dimension used by the synthetic semantic corpus. */
export const VECTOR_DIMENSION = 8

/** Hits retained for quality and token measurements. */
export const TOP_K = 10

/** Filler vocabulary; deliberately excludes the planted lexical terms. */
const FILLER = [
  'alpha', 'bravo', 'charlie', 'delta', 'echo', 'foxtrot', 'golf', 'hotel',
  'india', 'juliet', 'kilo', 'lima', 'mike', 'november', 'oscar', 'papa',
  'quebec', 'romeo', 'sierra', 'tango', 'uniform', 'victor', 'whiskey',
  'xray', 'yankee', 'zulu', 'build', 'cache', 'dispatch', 'editor', 'filter',
  'graph', 'handler', 'index', 'journal', 'kernel', 'loader', 'module',
] as const

/** Rare planted terms: they appear only in their lexical target documents. */
const LEXICAL_TERMS = [
  'zorblat', 'quibblewick', 'flarnge', 'mimswell', 'trundlebot',
  'grinshaw', 'wobblefin', 'plimpsett', 'drattle', 'shimmergast',
] as const

/** Lexical queries: each term names four planted documents. */
const LEXICAL_QUERIES = LEXICAL_TERMS.length

/** Semantic queries: each category owns 25 target documents. */
const SEMANTIC_QUERIES = CATEGORIES

/** One deterministic document in the synthetic corpus. */
export interface SyntheticRetrievalDocument {
  /** Stable document identity. */
  readonly id: string
  /** Workspace-relative display path. */
  readonly path: string
  /** Searchable text. */
  readonly text: string
  /** Synthetic dense vector; filler documents share one uniform unit vector. */
  readonly vector: readonly number[]
  /** Category index for semantic relevance, or `-1` for filler. */
  readonly category: number
  /** Planted lexical term, or `undefined` for a non-lexical document. */
  readonly lexicalTerm: string | undefined
}

/** One query with its relevance set and synthetic query vector. */
export interface SyntheticRetrievalQuery {
  /** Search text. */
  readonly text: string
  /** Planted lexical term, or `undefined` for a semantic query. */
  readonly lexicalTerm: string | undefined
  /** Relevant document ids. */
  readonly relevant: readonly string[]
  /** Synthetic query vector for semantic/hybrid modes. */
  readonly vector: readonly number[]
}

function categoryVector(category: number): number[] {
  return Array.from({ length: VECTOR_DIMENSION }, (_, index) => index === category % VECTOR_DIMENSION ? 1 : 0)
}

/** Uniform unit vector for documents with no semantic category. */
const UNIFORM_VECTOR = Array.from({ length: VECTOR_DIMENSION }, () => 1 / Math.sqrt(VECTOR_DIMENSION))

/** Words per document, approximating a small source file without multi-KB fixtures. */
export const DOCUMENT_WORDS = 80

function fillerWords(seed: number): string[] {
  const words: string[] = []
  for (let word = 0; word < DOCUMENT_WORDS; word += 1) words.push(FILLER[(seed * 7 + word * 3) % FILLER.length] as string)
  return words
}

/**
 * Build the fixed corpus.
 *
 * The first 250 documents are lexical targets: each of ten rare terms appears
 * in 25 documents, never in filler. The next 200 are semantic targets: each
 * of eight categories owns 25 documents whose vector is the category basis;
 * their text does NOT contain the semantic query word. Remaining documents are
 * unrelated filler.
 * @returns documents in stable id order.
 */
export function buildCorpus(): SyntheticRetrievalDocument[] {
  const documents: SyntheticRetrievalDocument[] = []
  for (let index = 0; index < CORPUS_DOCUMENTS; index += 1) {
    const lexical = index < LEXICAL_TERMS.length * 25 ? LEXICAL_TERMS[index % LEXICAL_TERMS.length] as string : undefined
    const semanticIndex = index >= LEXICAL_TERMS.length * 25 && index < LEXICAL_TERMS.length * 25 + CATEGORIES * 25
      ? index - LEXICAL_TERMS.length * 25
      : undefined
    const category = semanticIndex === undefined ? -1 : semanticIndex % CATEGORIES
    const words = fillerWords(index)
    if (lexical !== undefined) words[DOCUMENT_WORDS / 2] = lexical
    if (semanticIndex !== undefined) words[DOCUMENT_WORDS / 2] = `category-${String(category % CATEGORIES)}`
    documents.push({
      id: `doc-${String(index)}`,
      path: `src/module-${String(index % 20)}/file-${String(index)}.ts`,
      text: `file-${String(index)} ${words.join(' ')}`,
      vector: category === -1 ? UNIFORM_VECTOR : categoryVector(category),
      category,
      lexicalTerm: lexical,
    })
  }
  return documents
}

/**
 * Build the fixed queries.
 *
 * Lexical queries name one planted term and target its 25 documents.
 * Semantic queries name a word that never appears in their target text and
 * target the 25 documents of one category, with the category vector supplied
 * by the caller.
 * @returns queries in stable order.
 */
export function buildQueries(): SyntheticRetrievalQuery[] {
  const queries: SyntheticRetrievalQuery[] = []
  for (let index = 0; index < LEXICAL_QUERIES; index += 1) {
    const term = LEXICAL_TERMS[index] as string
    const relevant: string[] = []
    for (let document = index; document < LEXICAL_TERMS.length * 25; document += LEXICAL_TERMS.length) {
      relevant.push(`doc-${String(document)}`)
    }
    queries.push({ text: term, lexicalTerm: term, relevant, vector: UNIFORM_VECTOR })
  }
  for (let index = 0; index < SEMANTIC_QUERIES; index += 1) {
    const category = index % CATEGORIES
    const relevant: string[] = []
    const start = LEXICAL_TERMS.length * 25 + category
    const end = LEXICAL_TERMS.length * 25 + CATEGORIES * 25
    for (let document = start; document < end; document += CATEGORIES) relevant.push(`doc-${String(document)}`)
    queries.push({
      text: `meaning-of-category-${String(category)}`,
      lexicalTerm: undefined,
      relevant,
      vector: categoryVector(category),
    })
  }
  return queries
}

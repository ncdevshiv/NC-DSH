/**
 * Column encoding shared by every NC-AG repository: JSON columns are stored
 * as text so the physical schema stays inspectable with plain SQLite tooling.
 * @module @deepseek-ai/dsh-ncag-store/columns
 */

/**
 * Encode one JSON value for a text column.
 * @param value - the value to encode.
 * @returns the JSON text.
 */
export function encodeJson(value: unknown): string {
  return JSON.stringify(value)
}

/**
 * Decode one JSON text column.
 * @param text - stored text, or null when the column is nullable and unset.
 * @param fallback - value used for a null column; omitted means the column is required.
 * @returns the decoded value.
 */
export function decodeJson<T>(text: string | null, fallback?: T): T {
  if (text === null) {
    if (fallback === undefined) throw new Error('ncag: required JSON column is null')
    return fallback
  }
  return JSON.parse(text) as T
}

/**
 * Encode a boolean for an INTEGER column.
 * @param value - the boolean.
 * @returns 1 for true, 0 for false.
 */
export function encodeBool(value: boolean): number {
  return value ? 1 : 0
}

/**
 * Decode an INTEGER column holding a boolean.
 * @param value - the stored integer.
 * @returns whether it is nonzero.
 */
export function decodeBool(value: number): boolean {
  return value !== 0
}

/**
 * Decode a nullable INTEGER column.
 * @param value - the stored integer or null.
 * @returns the same value.
 */
export function decodeNullableNumber(value: number | null): number | null {
  return value
}



/**
 * Adopt a statement result as the caller's declared row type. The prepared
 * statement owner declares the column shape; this single conversion keeps the
 * SDK's generic record type out of every repository.
 * @param rows - rows returned by a prepared statement.
 * @returns the same rows typed as the caller's row interface.
 */
export function asRows<T>(rows: unknown): T[] {
  return rows as T[]
}

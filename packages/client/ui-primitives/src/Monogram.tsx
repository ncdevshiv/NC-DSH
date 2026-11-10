// Monogram: a decorative initials mark derived from a name the caller already
// renders as text. It draws no brand asset and claims none — two providers with
// the same initials share a mark, and the adjacent label is what identifies
// them. `AccountAvatar` in the account settings section is the picture case;
// this is the no-picture case.

import type { CSSProperties, ReactNode } from 'react'
import clsx from 'clsx'
import css from './Monogram.module.css'

/** Letterform height as a fraction of the mark edge, so all sizes read alike. */
const LETTER_SCALE = 0.42

/**
 * Derive the initials a mark draws from a name.
 *
 * Words are whitespace runs; a parenthetical qualifier is dropped because it
 * describes a variant rather than the subject ("OpenAI Codex (legacy)" marks as
 * `OC`). One word contributes its first letter, several contribute the first
 * letters of the first and last. Characters are matched by Unicode class, so a
 * non-Latin name yields its own leading character rather than a broken surrogate
 * pair, a name beginning with a symbol marks from its first real character, and
 * a symbol-only name yields nothing.
 * @param name - the name as the caller displays it; never localized here.
 * @returns up to two upper-case characters, or an empty string when the name
 *   carries no letter or digit.
 */
export function monogramLetters(name: string): string {
  const words = name.split(/\s+/u).filter(word => word.length > 0 && !/^[(\[]/u.test(word))
  if (words.length === 0) return ''
  const leading = (word: string): string => word.match(/[\p{L}\p{N}]/u)?.[0] ?? ''
  const parts = words.length === 1
    ? [leading(words[0]!)]
    : [leading(words[0]!), leading(words[words.length - 1]!)]
  return parts.join('').toUpperCase()
}

/**
 * Render a decorative initials mark.
 * @param props.name - the subject's name, taken verbatim; the mark is `aria-hidden`
 *   because the label beside it carries the identity.
 * @param props.size - mark edge in px (default 24).
 * @param props.className - extra class for layout placement.
 * @returns the mark, or nothing when {@link monogramLetters} finds no character to draw.
 */
export function Monogram({ name, size = 24, className }: {
  // `| undefined` so a caller can forward an optional prop straight through
  // under exactOptionalPropertyTypes.
  name: string
  size?: 20 | 24 | 32 | undefined
  className?: string | undefined
}): ReactNode {
  const letters = monogramLetters(name)
  if (letters.length === 0) return null
  const style = {
    '--monogram-edge': `${String(size)}px`,
    '--monogram-letter': `${String(Math.round(size * LETTER_SCALE))}px`,
  } as CSSProperties
  return <span className={clsx(css.monogram, className)} style={style} aria-hidden="true">{letters}</span>
}

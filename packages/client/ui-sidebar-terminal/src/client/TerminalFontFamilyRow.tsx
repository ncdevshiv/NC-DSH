/**
 * Terminal font-family row registered into the General section item slot:
 * title + terminal-only description + a select over the offered stacks. The
 * select shows a readable face name while the stored value is the full stack,
 * because that is what xterm consumes.
 */
import type { PropsLocale, PropsRuntime, PropsStore } from '@deepseek-ai/dsh-client-ui-slots'
import { TERMINAL_FONT_FAMILIES } from '../terminal-settings.ts'
import type { createTerminalTypographyStore } from './settings-store.ts'
import { fontFamilyLabel } from './settings-locales.ts'
import css from './TerminalTypographyRows.module.css'

/** Injected business face: the preference write (t rides the standard locale seat). */
export interface TerminalFontFamilyRowInjected {
  /** Change the terminal font family stack. */
  setFontFamily: (family: string) => void
}

/** Full component props: runtime share + store share + locale seat + injected face. */
export type TerminalFontFamilyRowComponentProps =
  PropsRuntime<'settings.general.item'> & PropsStore<ReturnType<typeof createTerminalTypographyStore>>
  & PropsLocale<'settings.terminal'> & TerminalFontFamilyRowInjected

/**
 * Render the terminal font-family row.
 * @param props - composed slot props.
 * @returns the row element tree.
 */
export function TerminalFontFamilyRow({ t, setFontFamily, useStore }: TerminalFontFamilyRowComponentProps) {
  const fontFamily = useStore(s => s.fontFamily)
  return (
    <div className={css.fontSizeRow}>
      <div className={css.fontSizeRowText}>
        <div className={css.fontSizeRowTitle}>{t('fontFamily.title')}</div>
        <div className={css.fontSizeRowDesc}>{t('fontFamily.description')}</div>
      </div>
      <div className={css.fontSizeRowControl}>
        <select
          className={css.fontFamilySelect}
          value={fontFamily}
          onChange={(event) => { setFontFamily(event.target.value) }}
        >
          {TERMINAL_FONT_FAMILIES.map(family => (
            <option key={family} value={family}>{fontFamilyLabel(family)}</option>
          ))}
        </select>
      </div>
    </div>
  )
}

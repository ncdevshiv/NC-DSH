/**
 * Terminal font-size row registered into the General section item slot: title
 * + terminal-only description + stepper pill with a px unit label. The displayed
 * value follows the persisted setting, never the click echo.
 */
import {
  IconChevronDownOutlineRegular, IconChevronUpOutlineRegular,
} from '@deepseek-ai/dsh-client-ui-primitives'
import type { PropsLocale, PropsRuntime, PropsStore } from '@deepseek-ai/dsh-client-ui-slots'
import { TERMINAL_FONT_SIZE_MAX, TERMINAL_FONT_SIZE_MIN } from '../terminal-settings.ts'
import type { createTerminalTypographyStore } from './settings-store.ts'
import css from './TerminalTypographyRows.module.css'

/** Injected business face: the preference write (t rides the standard locale seat). */
export interface TerminalFontSizeRowInjected {
  /** Change the terminal font size (integer px within the accepted range). */
  setFontSize: (px: number) => void
}

/** Full component props: runtime share + store share + locale seat + injected face. */
export type TerminalFontSizeRowComponentProps =
  PropsRuntime<'settings.general.item'> & PropsStore<ReturnType<typeof createTerminalTypographyStore>>
  & PropsLocale<'settings.terminal'> & TerminalFontSizeRowInjected

/**
 * Render the terminal font-size row.
 * @param props - composed slot props.
 * @returns the row element tree.
 */
export function TerminalFontSizeRow({ t, setFontSize, useStore }: TerminalFontSizeRowComponentProps) {
  const fontSize = useStore(s => s.fontSize)
  return (
    <div className={css.fontSizeRow}>
      <div className={css.fontSizeRowText}>
        <div className={css.fontSizeRowTitle}>{t('fontSize.title')}</div>
        <div className={css.fontSizeRowDesc}>{t('fontSize.description')}</div>
      </div>
      <div className={css.fontSizeRowControl}>
        <div className={css.fontSizeStepper}>
          <span className={css.fontSizeValue}>{fontSize}</span>
          <span className={css.fontSizeArrows}>
            <button
              type="button"
              className={css.fontSizeArrow}
              aria-label={t('fontSize.increase')}
              disabled={fontSize >= TERMINAL_FONT_SIZE_MAX}
              onClick={() => { setFontSize(fontSize + 1) }}
            >
              <IconChevronUpOutlineRegular size={9} />
            </button>
            <button
              type="button"
              className={css.fontSizeArrow}
              aria-label={t('fontSize.decrease')}
              disabled={fontSize <= TERMINAL_FONT_SIZE_MIN}
              onClick={() => { setFontSize(fontSize - 1) }}
            >
              <IconChevronDownOutlineRegular size={9} />
            </button>
          </span>
        </div>
        <span className={css.fontSizeUnit}>{t('fontSize.unit')}</span>
      </div>
    </div>
  )
}

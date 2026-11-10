/** Locale bundles for the terminal typography settings rows. */
import type {} from '@deepseek-ai/dsh-client-ui-slots'

declare module '@deepseek-ai/dsh-client-ui-slots' {
  interface LocaleNamespaceMap {
    /** Terminal typography copy owned by this plugin. */
    'settings.terminal': TerminalSettingsLocaleKey
  }
}

/** Locale keys the rows render. */
export type TerminalSettingsLocaleKey =
  | 'fontFamily.title' | 'fontFamily.description'
  | 'fontSize.title' | 'fontSize.description' | 'fontSize.unit'
  | 'fontSize.increase' | 'fontSize.decrease'

/** English copy. */
export const en: Record<TerminalSettingsLocaleKey, string> = {
  'fontFamily.title': 'Terminal font',
  'fontFamily.description': 'Typeface used by the terminal panel.',
  'fontSize.title': 'Terminal font size',
  'fontSize.description': 'Only affects the terminal panel.',
  'fontSize.unit': 'px',
  'fontSize.increase': 'Increase terminal font size',
  'fontSize.decrease': 'Decrease terminal font size',
}

/** Simplified Chinese copy. */
export const zh: Record<TerminalSettingsLocaleKey, string> = {
  'fontFamily.title': '终端字体',
  'fontFamily.description': '终端面板使用的字体。',
  'fontSize.title': '终端字号',
  'fontSize.description': '仅影响终端面板。',
  'fontSize.unit': 'px',
  'fontSize.increase': '增大终端字号',
  'fontSize.decrease': '减小终端字号',
}

/**
 * Display name for one font stack, so the select shows a readable face.
 * @param family - the stored stack, as the setting writes it.
 * @returns the first face in the stack, without quotes.
 */
export function fontFamilyLabel(family: string): string {
  const first = family.split(',')[0]?.trim() ?? family
  return first.replace(/^["']|["']$/gu, '')
}

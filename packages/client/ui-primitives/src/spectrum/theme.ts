// DeepSeek-branded Spectrum theme: the stock defaultTheme supplies every
// variable this port has not remapped yet; the override module restates the
// semantic core as var() references into the --dsw-* token sheets, so the
// ui-theme stylesheets stay the single color authority and scheme flips keep
// riding body[data-ds-dark-theme] instead of Provider's own class logic.
// The same override class is installed for both schemes on purpose: resolved
// colors are identical, and the cascade under the attribute decides values.

import { defaultTheme } from '@adobe/react-spectrum'
import type { Theme } from '@adobe/react-spectrum/Provider'
import vars from './spectrum-vars.module.css'

/** Theme passed to Provider at the application root. */
export const deepseekTheme: Theme = {
  ...defaultTheme,
  light: { ...defaultTheme.light, ...vars },
  dark: { ...defaultTheme.dark, ...vars },
}

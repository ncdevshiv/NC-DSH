// SpectrumSurface: application-root Provider for the Spectrum component
// layer. Lives in the static primitives channel so dynamic plugin bundles
// never request @adobe/react-spectrum through the module table — they consume
// Spectrum only through these atoms. No colorScheme prop on purpose: both
// scheme slots of the DeepSeek theme carry identical var()-based overrides,
// so resolved colors follow body[data-ds-dark-theme] regardless of which
// scheme class Provider stamps.

import { Provider } from '@adobe/react-spectrum'
import type { ReactNode } from 'react'
import { deepseekTheme } from './theme.ts'

/**
 * Mount the Spectrum provider layer beneath the app root.
 * @param props.children - application subtree rendered under Provider.
 * @returns the provider element wrapping the subtree.
 */
export function SpectrumSurface({ children }: { children: ReactNode }) {
  return <Provider theme={deepseekTheme}>{children}</Provider>
}

// SpectrumAppFrame: root registration wrapper seating the Spectrum provider
// layer around the whole frame. Keeps AppFrame's exact composed props; adds
// no state and renders nothing of its own beyond the provider element.
import type { ComponentProps, ReactElement } from 'react'
import { SpectrumSurface } from '@deepseek-ai/dsh-client-ui-primitives'
import { AppFrame } from './AppFrame.tsx'

/**
 * Wrap the frame in the Spectrum provider layer.
 * @param props - the four framework shares AppFrame composes against.
 * @returns the frame under the Spectrum surface.
 */
export function SpectrumAppFrame(props: ComponentProps<typeof AppFrame>): ReactElement {
  return (
    <SpectrumSurface>
      <AppFrame {...props} />
    </SpectrumSurface>
  )
}

/** Operation feedback hosted outside the Plugins panel's lifetime. */
import type { ReactNode } from 'react'
import { IconWarningOutlineRegular, Toast } from '@deepseek-ai/dsh-client-ui-primitives'
import type { InjectFace, PropsLocale } from '@deepseek-ai/dsh-client-ui-slots'
import type { PluginManagerFace } from './manager-store.ts'
import { noticeText, toastHoldMs } from './presentation.ts'

/** The controller's shared notice source and dismissal action. */
export type PluginManagerToastFace = Pick<PluginManagerFace, 'dismissNotice'> & {
  hooks: Pick<PluginManagerFace['hooks'], 'pluginManager'>
}

/** Render inputs bound from the shared controller and plugin dictionary. */
export type PluginManagerToastProps = InjectFace<PluginManagerToastFace> & PropsLocale<'pluginManager'>

/**
 * Show an action's outcome even after navigation leaves the Plugins panel: an
 * install, enable, or removal can outlive the page that requested it, and only
 * a toast held by the shell survives that close.
 * @param props - shared notice hook, dismissal action, and locale seat.
 * @returns the current notice toast, or null while no notice is pending.
 */
export function PluginManagerToast({ usePluginManager, dismissNotice, t }: PluginManagerToastProps): ReactNode {
  const notice = usePluginManager(state => state.notice)
  if (notice === null) return null
  const text = noticeText(notice, t)
  return (
    <Toast
      key={notice.seq}
      text={text}
      icon={<IconWarningOutlineRegular />}
      holdMs={toastHoldMs(text)}
      onDone={dismissNotice}
    />
  )
}

/**
 * Post-rewind restore notice: folds one wire restore report into the one
 * composer notice line the child session surfaces after opening.
 * @module dsh-client-ui-conversation/restore-notice
 */

import type { ForkRestoreReport } from '@deepseek-ai/dsh-client-runtime/client'
import type { ChatViewSlotProps } from '../contract/slots.ts'

/** One composer notice derived from a rewind's restore report. */
export interface RestoreNotice {
  readonly level: 'info' | 'error'
  readonly text: string
}

/**
 * Fold one {@link ForkRestoreReport} into a composer notice. Conflicts are
 * error-level (the user must see that the rewind is partial); a skipped
 * restore and a clean pass are informational, with shell and un-restorable
 * counts appended as honest caveats.
 * @param report - the host's restore result.
 * @param t - the conversation locale seat.
 * @returns the notice level and text.
 */
export function restoreNotice(report: ForkRestoreReport, t: ChatViewSlotProps['t']): RestoreNotice {
  if (report.skipped === 'source-running') {
    return { level: 'info', text: t('restore.skipped.running') }
  }
  if (report.skipped === 'no-cwd') {
    return { level: 'info', text: t('restore.skipped.noCwd') }
  }
  if (report.conflicts.length > 0) {
    return {
      level: 'error',
      text: t('restore.conflicts', {
        count: report.conflicts.length,
        paths: report.conflicts.slice(0, 3).join(', '),
      }),
    }
  }
  let text = t('restore.summary', { restored: report.restored })
  if (report.shell.count > 0) {
    text += ` ${t('restore.shell', { count: report.shell.count, names: report.shell.names.join(', ') })}`
  }
  if (report.notRestorable.count > 0) {
    text += ` ${t('restore.notRestorable', { names: report.notRestorable.toolNames.join(', ') })}`
  }
  return { level: 'info', text }
}

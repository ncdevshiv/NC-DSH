/**
 * The automations entry in the Settings panel.
 *
 * Scheduled tasks already have a full management surface — the searchable list,
 * the timing editor, the delivery history, the cancellation flow — composed as
 * the `schedules` main panel. This section is that panel's Second entry point:
 * it explains what schedules are and opens the real panel on the user's action,
 * rather than re-implementing a subset of it where the two could drift.
 *
 * The section renders nothing until the panel it opens exists. A deployment
 * that composed no schedule page shows no broken entry.
 */

import type { InjectFace, PropsLocale, PropsRuntime } from '@deepseek-ai/dsh-client-ui-slots'
import { Button } from '@deepseek-ai/dsh-client-ui-primitives'
import type {} from '@deepseek-ai/dsh-client-ui-settings/client'
import type {} from '@deepseek-ai/dsh-client-ui-layout/client'
import css from './AutomationsSection.module.css'

/** Injected actions for the automations entry. */
export interface AutomationsSectionInjected {
  /** Whether the schedule management panel is composed. */
  readonly hasPanel: () => boolean
  /** Open the schedule management panel and close the Settings panel. */
  readonly openPanel: () => void
}

/** Full component props: runtime share plus the injected actions. */
export type AutomationsSectionProps =
  PropsRuntime<'settings.section'>
  & PropsLocale<'schedule.manager'>
  & InjectFace<AutomationsSectionInjected>

/**
 * Render the automations entry.
 * @param props - the settings model's injected actions and localized copy.
 * @returns the entry, or nothing while the schedule panel is not composed.
 */
export function AutomationsSection({ hasPanel, openPanel, close, t }: AutomationsSectionProps) {
  if (!hasPanel()) return null
  return (
    <div className={css.section}>
      <div className={css.text}>
        <div className={css.title}>{t('nav')}</div>
        <p className={css.description}>{t('section.description')}</p>
      </div>
      <div className={css.actions}>
        <Button
          variant="primary"
          size="sm"
          onClick={() => {
            openPanel()
            close()
          }}
        >
          {t('section.open')}
        </Button>
      </div>
    </div>
  )
}

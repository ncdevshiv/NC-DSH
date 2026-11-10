/** NC-AG settings fields inside the shared settings card. */

import type { PropsLocale } from '@deepseek-ai/dsh-client-ui-slots'
import { SettingsValueField, type SettingsFieldState } from '@deepseek-ai/dsh-client-ui-primitives'
import type { NcagCardFace, NcagCardState } from './ncag-card-controller.ts'
import css from './NcagFields.module.css'

/** Plain state and edit callbacks supplied by the owning card. */
export type NcagFieldsProps = PropsLocale<'settings.ncag'>
  & Pick<NcagCardFace, 'edit' | 'resetField'>
  & { state: NcagCardState }

/**
 * Render the model overrides, price overrides, and limits.
 * @param props - Locale, staged fields, and edit callbacks.
 * @returns The NC-AG settings sections.
 */
export function NcagFields(props: NcagFieldsProps) {
  const { t, state } = props
  const disabled = !state.writable || state.saving
  const field = (
    id: string,
    label: string,
    key: keyof NcagCardState,
    numeric: boolean,
    hint: string,
  ) => (
    <div className={css.field}>
      <SettingsValueField id={id} label={label} help={{ label, content: <p>{hint}</p> }}
        overriddenLabel={t('overridden')} resetLabel={t('reset')}
        invalidLabel={numeric ? t('ncagUnlimitedHint') : t('ncagModelPlaceholder')}
        numeric={numeric} disabled={disabled} {...(state[key] as SettingsFieldState)}
        onEdit={(text) => { props.edit(String(key), text) }} onReset={() => { props.resetField(String(key)) }} />
    </div>
  )
  return (
    <>
      <section className={css.section}>
        <h3 className={css.heading}>{t('ncagModelsTitle')}</h3>
        <p className={css.help}>{t('ncagModelsHelp')}</p>
        {field('plugin-config-ncag-fast', t('ncagFastModel'), 'fastModel', false, t('ncagModelPlaceholder'))}
        {field('plugin-config-ncag-standard', t('ncagStandardModel'), 'standardModel', false, t('ncagModelPlaceholder'))}
        {field('plugin-config-ncag-reasoning', t('ncagReasoningModel'), 'reasoningModel', false, t('ncagModelPlaceholder'))}
      </section>
      <section className={css.section}>
        <h3 className={css.heading}>{t('ncagPricingTitle')}</h3>
        <p className={css.help}>{t('ncagPricingHelp')}</p>
        {field('plugin-config-ncag-pricing', t('ncagPricingField'), 'pricingOverrides', false, t('ncagPricingHelp'))}
      </section>
      <section className={css.section}>
        <h3 className={css.heading}>{t('ncagLimitsTitle')}</h3>
        <p className={css.help}>{t('ncagLimitsHelp')}</p>
        <div className={css.limits}>
          {field('plugin-config-ncag-agents', t('ncagMaxConcurrentAgents'), 'uiMaxConcurrentAgents', true, t('ncagUnlimitedHint'))}
          {field('plugin-config-ncag-helpers', t('ncagMaxAgentsPerTask'), 'uiMaxAgentsPerTask', true, t('ncagUnlimitedHint'))}
          {field('plugin-config-ncag-steps', t('ncagMaxSteps'), 'uiMaxStepsPerTask', true, t('ncagDefaultHint'))}
          {field('plugin-config-ncag-tokens', t('ncagMaxOutputTokens'), 'uiMaxOutputTokens', true, t('ncagDefaultHint'))}
          {field('plugin-config-ncag-cost', t('ncagMaxCost'), 'uiMaxCostUsd', true, t('ncagUnlimitedHint'))}
        </div>
      </section>
    </>
  )
}

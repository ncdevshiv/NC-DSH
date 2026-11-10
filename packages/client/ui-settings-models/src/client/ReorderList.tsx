/**
 * The ordered row list the Models rail uses: one row per provider, reorderable
 * by pointer drag and by its arrow buttons, with a status dot, an optional
 * caption, a selection button, an enable switch, and any number of glyph
 * actions.
 *
 * Every label arrives resolved: a row without a selection button, switch, or
 * action renders without that control, so the list states no product facts of
 * its own beyond the glyph each action id draws.
 */

import { useId } from 'react'
import type { ReactNode } from 'react'
import {
  IconChevronDownOutlineRegular, IconChevronUpOutlineRegular, IconEditOutlineRegular,
  IconRefreshOutlineRegular, IconTrashOutlineRegular, StateDot, Switch,
} from '@deepseek-ai/dsh-client-ui-primitives'
import type { StateDotState } from '@deepseek-ai/dsh-client-ui-primitives'
import type { ModelsKey } from './locales.ts'
import { useDragOrder } from './useDragOrder.ts'
import styles from './ModelsSection.module.css'

/** The glyph actions a rail row may offer. */
export type RowActionId = 'fetch' | 'edit' | 'delete'

/** One glyph action a row offers. */
export interface RowAction {
  readonly id: RowActionId
  /** Accessible name and hover text. */
  readonly label: string
  /** Why the action is unavailable; overrides the hover text when present. */
  readonly blocked?: string | undefined
  /** Whether the action refuses input. */
  readonly disabled?: boolean | undefined
  /** Whether the action deletes rather than changes something. */
  readonly danger?: boolean | undefined
  /** Hover text where it differs from the accessible name. */
  readonly title?: string | undefined
}

/** The enable switch one row shows. */
export interface RowToggle {
  /** Accessible name of the switch; stable across both states. */
  readonly label: string
  /** Hover text naming what the switch does next. */
  readonly title?: string | undefined
  readonly checked: boolean
  readonly disabled?: boolean | undefined
}

/** The glyph each action id draws. */
const ROW_ACTION_ICON: Record<RowActionId, ReactNode> = {
  fetch: <IconRefreshOutlineRegular size={14} />,
  edit: <IconEditOutlineRegular size={14} />,
  delete: <IconTrashOutlineRegular size={14} />,
}

/** One row {@link ReorderList} renders. */
export interface ReorderItem {
  /** Glyph actions the row offers, in render order. */
  readonly actions?: readonly RowAction[] | undefined
  /** Caption beside the label, e.g. the adapter's declaration tag. */
  readonly caption?: string | undefined
  /** Confirmed credential state shown as a dot beside the label. */
  readonly credential?: 'configured' | 'missing' | undefined
  /** Whether the row's selection button refuses input (a write for it is in flight). */
  readonly disabled?: boolean | undefined
  /** State dot semantic, or absent for a row that states nothing. */
  readonly dot?: StateDotState | undefined
  /** Whether every control on the row is held still (a refresh, a locked document, or a pending write). */
  readonly held?: boolean | undefined
  readonly id: string
  readonly label: string
  /** Whether the row takes a drag and shows its arrow buttons. */
  readonly movable?: boolean | undefined
  readonly selected?: boolean | undefined
  /** Accessible name of the row's selection button; absent when it is not selectable. */
  readonly selectLabel?: string | undefined
  /** Status the dot states, announced as the selection button's description. */
  readonly statusText?: string | undefined
  /** The row's enable switch; absent when the row carries no such state. */
  readonly toggle?: RowToggle | undefined
}

/** Props of {@link ReorderList}. */
export interface ReorderListProps<T extends ReorderItem> {
  /** Ids the move positions count against, in reorderable order. */
  order: readonly string[]
  /** Rows in listed order. */
  items: readonly T[]
  /** Move the row to position `to` within `order`. */
  onMove: (item: T, to: number) => void
  /** Run one of the row's glyph actions. */
  onRowAction: (item: T, action: RowAction) => void
  /** Make the row the one the page shows. */
  onSelect: (item: T) => void
  /** Flip the row's enable switch. */
  onToggle: (item: T) => void
  /** Section copy. */
  t: (key: ModelsKey) => string
  /** Optional expanded body rendered under one row, outside its drag surface. */
  renderDetail?: (item: T) => ReactNode
}

/**
 * Render one reorderable list.
 * @param props - the rows, their reorderable order, and the row actions.
 * @returns the list.
 */
export function ReorderList<T extends ReorderItem>(props: ReorderListProps<T>): ReactNode {
  const { items, order, onMove, onRowAction, onSelect, onToggle, t, renderDetail } = props
  const drag = useDragOrder<T>(order, onMove)
  const statusBase = useId()

  const body = (item: T): ReactNode => {
    const statusId = item.statusText === undefined ? undefined : `${statusBase}-${item.id}`
    const content = (
      <>
        {item.dot === undefined ? null : <StateDot state={item.dot} size={8} />}
        <span className={styles['reorderLabel']}>{item.label}</span>
        {item.caption === undefined ? null : <span className={styles['reorderCaption']}>{item.caption}</span>}
        {item.credential === undefined
          ? null
          : (
            <span
              role="img"
              aria-label={t(item.credential === 'configured' ? 'credentialConfigured' : 'credentialMissing')}
              title={t(item.credential === 'configured' ? 'credentialConfigured' : 'credentialMissing')}
              className={`${styles['credentialDot']} ${item.credential === 'configured'
                ? styles['credentialDotConfigured']
                : styles['credentialDotMissing']}`}
            />
          )}
        {item.statusText === undefined
          ? null
          : <span className={styles['hiddenLabel']} id={statusId}>{item.statusText}</span>}
      </>
    )
    return item.selectLabel === undefined
      ? <span className={styles['reorderStatic']}>{content}</span>
      : (
        <button
          type="button"
          className={styles['reorderSelect']}
          aria-label={item.selectLabel}
          title={item.selectLabel}
          {...statusId === undefined ? {} : { 'aria-describedby': statusId }}
          {...item.selected === true ? { 'aria-current': 'true' as const } : {}}
          disabled={item.disabled === true}
          onClick={() => { onSelect(item) }}
        >{content}
        </button>
      )
  }

  return (
    <ul className={styles['reorderList']}>
      {items.map((item) => {
        const position = order.indexOf(item.id)
        const up = `${t('moveUp')} ${item.label}`
        const down = `${t('moveDown')} ${item.label}`
        return (
          <li key={item.id} className={styles['reorderItem']}>
            <div
              className={[
                styles['reorderRow'],
                item.selected === true ? styles['reorderRowSelected'] : '',
                drag.isDragging(item) ? styles['reorderRowDragging'] : '',
              ].filter(Boolean).join(' ')}
              {...item.movable === true ? drag.rowProps(item) : {}}
            >
              {body(item)}
              {item.toggle === undefined
                ? null
                : (
                  <span className={styles['reorderToggle']}>
                    <Switch
                      checked={item.toggle.checked}
                      label={item.toggle.label}
                      title={item.toggle.title}
                      disabled={item.toggle.disabled === true}
                      onChange={() => { onToggle(item) }}
                    />
                  </span>
                )}
              {(item.actions ?? []).map(action => (
                <button
                  key={action.id}
                  type="button"
                  className={`${styles['reorderAction']} ${action.danger === true ? styles['reorderActionDanger'] : ''}`}
                  aria-label={action.label}
                  title={action.blocked ?? action.title ?? action.label}
                  disabled={action.disabled === true}
                  onClick={() => { onRowAction(item, action) }}
                >{ROW_ACTION_ICON[action.id]}
                </button>
              ))}
              {item.movable === true
                ? (
                  <span className={styles['reorderArrows']}>
                    <button
                      type="button"
                      className={styles['reorderAction']}
                      aria-label={up}
                      title={up}
                      disabled={item.held === true || position <= 0}
                      onClick={() => { onMove(item, position - 1) }}
                    ><IconChevronUpOutlineRegular size={14} />
                    </button>
                    <button
                      type="button"
                      className={styles['reorderAction']}
                      aria-label={down}
                      title={down}
                      disabled={item.held === true || position >= order.length - 1}
                      onClick={() => { onMove(item, position + 1) }}
                    ><IconChevronDownOutlineRegular size={14} />
                    </button>
                  </span>
                )
                : null}
            </div>
            {renderDetail?.(item)}
          </li>
        )
      })}
    </ul>
  )
}

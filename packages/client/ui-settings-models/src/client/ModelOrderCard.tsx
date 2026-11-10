/**
 * The order one route serves its models in: the model ids the selected provider
 * resolves, listed in served order and reorderable by pointer drag or arrow
 * buttons.
 *
 * A move writes the profile's `modelOrder` directly rather than joining the
 * editor's draft, so reordering never rewrites the model list it orders.
 */

import type { ReactNode } from 'react'
import { IconChevronDownOutlineRegular, IconChevronUpOutlineRegular } from '@deepseek-ai/dsh-client-ui-primitives'
import type { ModelsKey } from './locales.ts'
import { useDragOrder } from './useDragOrder.ts'
import styles from './ModelsSection.module.css'

/** One served model, addressed by its id. */
interface ModelRowRef {
  readonly id: string
}

/** Props of {@link ModelOrderCard}. */
export interface ModelOrderCardProps {
  /** Model ids in the order the route serves them; the card lists none of its own. */
  ids: readonly string[]
  /** Whether a row-level write for this provider is in flight. */
  pending: boolean
  /** Whether the settings document accepts writes. */
  readOnly: boolean
  /** Section copy. */
  t: (key: ModelsKey) => string
  /** Move the model to position `to` within `ids`. */
  onMove: (id: string, to: number) => void
}

/**
 * Render one route's served model order.
 * @param props - the model ids, the write state, and the move action.
 * @returns the order card.
 */
export function ModelOrderCard(props: ModelOrderCardProps): ReactNode {
  const { ids, onMove, pending, readOnly, t } = props
  const rows = ids.map((id): ModelRowRef => ({ id }))
  const drag = useDragOrder<ModelRowRef>(ids, (row, to) => { onMove(row.id, to) })
  const blocked = readOnly || pending
  return (
    <div className={styles['modelCatalog']}>
      <div className={styles['modelCatalogHeading']}>
        <p className={styles['modelCatalogTitle']}>{t('modelOrder')}</p>
        <p className={styles['modelCatalogMeta']}>{t('reorderHint')}</p>
      </div>
      <ul className={styles['reorderList']}>
        {rows.map((row, index) => (
          <li
            key={row.id}
            className={[
              styles['reorderRow'],
              drag.isDragging(row) ? styles['reorderRowDragging'] : '',
            ].filter(Boolean).join(' ')}
            {...drag.rowProps(row)}
          >
            <span className={styles['reorderStatic']}>
              <span className={styles['reorderLabel']}>{row.id}</span>
            </span>
            <span className={styles['reorderArrows']}>
              <button
                type="button"
                className={styles['reorderAction']}
                aria-label={`${t('moveUp')} ${row.id}`}
                title={`${t('moveUp')} ${row.id}`}
                disabled={blocked || index <= 0}
                onClick={() => { onMove(row.id, index - 1) }}
              ><IconChevronUpOutlineRegular size={14} />
              </button>
              <button
                type="button"
                className={styles['reorderAction']}
                aria-label={`${t('moveDown')} ${row.id}`}
                title={`${t('moveDown')} ${row.id}`}
                disabled={blocked || index >= rows.length - 1}
                onClick={() => { onMove(row.id, index + 1) }}
              ><IconChevronDownOutlineRegular size={14} />
              </button>
            </span>
          </li>
        ))}
      </ul>
    </div>
  )
}

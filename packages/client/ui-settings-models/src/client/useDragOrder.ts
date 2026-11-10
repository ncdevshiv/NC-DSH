/**
 * The pointer-drag half of row reordering, shared by the Models rail and the
 * model-order card: the row a drag started on and the handlers a reorderable row
 * takes.
 *
 * This is component-local state over nothing outside the component — it holds no
 * subscription, reads no service, and survives no remount. The dragged row lives
 * in React state rather than in `dataTransfer`, so a drop only needs the row it
 * lands on; `dataTransfer` is still written when the engine hands one over,
 * which Firefox requires to start a drag.
 */

import { useState } from 'react'
import type { DragEvent } from 'react'

/** One row a drag can move, identified by its position within `order`. */
export interface DragRow {
  readonly id: string
}

/** The drag wiring one reorderable row element takes. */
export type DragRowProps = {
  draggable: true
  onDragStart: (event: DragEvent<HTMLElement>) => void
  onDragOver: (event: DragEvent<HTMLElement>) => void
  onDragEnd: () => void
  onDrop?: (event: DragEvent<HTMLElement>) => void
}

/**
 * Track a row drag over an ordered list.
 * @param order - the reorderable ids, in listed order.
 * @param onMove - moves the dragged row to the position the row it lands on holds.
 * @returns the per-row handlers and whether a row is the one being moved.
 */
export function useDragOrder<T extends DragRow>(
  order: readonly string[],
  onMove: (row: T, to: number) => void,
): { rowProps: (row: T) => DragRowProps; isDragging: (row: T) => boolean } {
  const [dragged, setDragged] = useState<T | null>(null)
  return {
    isDragging: row => dragged?.id === row.id,
    rowProps: row => ({
      draggable: true,
      onDragStart: (event) => {
        setDragged(row)
        event.dataTransfer.setData('text/plain', row.id)
      },
      onDragOver: (event) => { event.preventDefault() },
      onDragEnd: () => { setDragged(null) },
      ...(dragged === null || dragged.id === row.id
        ? {}
        : {
          onDrop: (event: DragEvent<HTMLElement>) => {
            event.preventDefault()
            onMove(dragged, order.indexOf(row.id))
            setDragged(null)
          },
        }),
    }),
  }
}

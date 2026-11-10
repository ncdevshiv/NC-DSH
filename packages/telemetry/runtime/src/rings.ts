/**
 * Bounded ring buffer: fixed capacity, oldest entry overwritten first. The
 * tracing rings never grow and never touch the disk, so a busy process pays a
 * constant, allocation-free push on the hot path.
 */

/** One fixed-capacity FIFO with O(1) push. */
export class Ring<T> {
  private readonly items: (T | undefined)[]
  private next = 0
  private filled = 0

  /** @param capacity - maximum retained entries; must be a positive integer. */
  constructor(private readonly capacity: number) {
    this.items = new Array<T | undefined>(capacity)
  }

  /**
   * Append one entry, overwriting the oldest when full.
   * @param value - the entry to retain.
   */
  push(value: T): void {
    this.items[this.next] = value
    this.next = (this.next + 1) % this.capacity
    if (this.filled < this.capacity) this.filled += 1
  }

  /** @returns retained entries, oldest first. */
  toArray(): T[] {
    if (this.filled < this.capacity) return this.items.slice(0, this.filled) as T[]
    return [
      ...this.items.slice(this.next) as T[],
      ...this.items.slice(0, this.next) as T[],
    ]
  }

  /** @returns the retained entry count. */
  get size(): number {
    return this.filled
  }
}

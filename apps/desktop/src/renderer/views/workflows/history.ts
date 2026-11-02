// The canvas undo stack. It lives in memory and stops at 100 entries.

const LIMIT = 100

export class UndoStack<T> {
  private past: T[] = []
  private future: T[] = []

  get canUndo() {
    return this.past.length > 0
  }

  get canRedo() {
    return this.future.length > 0
  }

  push(previous: T) {
    this.past.push(previous)
    if (this.past.length > LIMIT) this.past.shift()
    this.future = []
  }

  undo(current: T): T | null {
    const previous = this.past.pop()
    if (previous === undefined) return null
    this.future.push(current)
    return previous
  }

  redo(current: T): T | null {
    const next = this.future.pop()
    if (next === undefined) return null
    this.past.push(current)
    if (this.past.length > LIMIT) this.past.shift()
    return next
  }

  clear() {
    this.past = []
    this.future = []
  }
}

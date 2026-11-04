/** A `Storage` that lives in memory only: what the demo uses where the app would write to the device. */
export class MemoryStorage implements Pick<Storage, 'getItem' | 'setItem' | 'removeItem'> {
  private items = new Map<string, string>()

  getItem(key: string) {
    return this.items.get(key) ?? null
  }

  setItem(key: string, value: string) {
    this.items.set(key, String(value))
  }

  removeItem(key: string) {
    this.items.delete(key)
  }
}

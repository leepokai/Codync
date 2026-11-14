import { Observable } from '../../lib/observable'
import { appendPage, emptyPage, errorText, replacePage, type MarketplacePage } from './market-models'

type Fetch<T> = (search: string, cursor: string | null) => Promise<{ items: T[]; next: string | null }>

/**
 * The connector list Extensions scrolls through (kit `ConnectorCatalog`): the rows loaded so far
 * and the registry cursor. The end of the list coming into view asks for more (`loadMore`).
 */
export class ConnectorCatalog<T> extends Observable {
  items: MarketplacePage<T> = emptyPage()
  /** The registry page after the ones loaded; null at the end. */
  cursor: string | null = null
  loading = true
  loadingMore = false
  error: string | null = null
  #request = 0
  #query = ''
  readonly #id: (item: T) => string
  readonly #fetch: Fetch<T>

  constructor(id: (item: T) => string, fetch: Fetch<T>) {
    super()
    this.#id = id
    this.#fetch = fetch
  }

  get hasMore() {
    return this.cursor !== null
  }

  /** The first registry page for `search`; answers to an older search are dropped. */
  async load(search: string) {
    const request = ++this.#request
    this.#query = search
    this.loading = true
    this.loadingMore = false
    this.changed()
    try {
      const first = await this.#fetch(search, null)
      if (request !== this.#request) return
      this.items = replacePage(first.items, this.#id)
      this.cursor = first.next
      this.error = null
    } catch (e) {
      if (request !== this.#request) return
      this.items = emptyPage()
      this.cursor = null
      this.error = errorText(e)
    } finally {
      if (request === this.#request) {
        this.loading = false
        this.changed()
      }
    }
  }

  /** The last search again, after an install changed what it lists. */
  reload() {
    return this.load(this.#query)
  }

  /**
   * The registry page at the cursor. It can bring nothing Codync can install; the cursor still moves
   * on, so `LoadMoreRow` asks again. A failure keeps the cursor and shows under the list.
   */
  async loadMore() {
    if (this.loading || this.loadingMore || !this.hasMore) return
    const request = this.#request
    this.loadingMore = true
    this.error = null
    this.changed()
    try {
      const more = await this.#fetch(this.#query, this.cursor)
      if (request !== this.#request) return
      this.items = appendPage(this.items, more.items, this.#id)
      this.cursor = more.next
    } catch (e) {
      if (request !== this.#request) return
      this.error = errorText(e)
    } finally {
      if (request === this.#request) {
        this.loadingMore = false
        this.changed()
      }
    }
  }

  /** Drops the answers still on their way (the view went away). */
  cancel() {
    this.#request++
  }
}

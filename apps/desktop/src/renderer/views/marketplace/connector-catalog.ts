import { Observable } from '../../lib/observable'
import { appendPage, clampPage, emptyPage, errorText, needsFetch, replacePage, type MarketplacePage } from './market-models'

type Fetch<T> = (search: string, cursor: string | null) => Promise<{ items: T[]; next: string | null }>

/**
 * The connector list Extensions pages through (kit `ConnectorCatalog`): the rows loaded so far, the
 * registry cursor and the page on screen. A page that isn't full asks the registry until it is, or
 * until the cursor ends.
 */
export class ConnectorCatalog<T> extends Observable {
  items: MarketplacePage<T> = emptyPage()
  /** The registry page after the ones loaded; null at the end. */
  cursor: string | null = null
  /** The page on screen. While the registry fills it, it can be the page past the loaded rows. */
  page = 0
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
      this.page = 0
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

  /** Turns to `next` at once; `fill` then loads its rows. */
  show(next: number, size: number) {
    if (this.loading) return
    this.page = clampPage(next, this.items.items.length, size, this.hasMore)
    this.changed()
  }

  /**
   * Asks the registry until the page on screen is full or the cursor ends. A page turned meanwhile
   * is filled by the same loop. If a page fails, the list goes back to the last page with rows.
   */
  async fill(size: number) {
    if (this.loading || this.loadingMore || !this.hasMore || !needsFetch(this.items.items.length, this.page, size)) return
    const request = this.#request
    this.loadingMore = true
    this.changed()
    try {
      while (this.cursor && needsFetch(this.items.items.length, this.page, size)) {
        const more = await this.#fetch(this.#query, this.cursor)
        if (request !== this.#request) return
        this.items = appendPage(this.items, more.items, this.#id)
        this.cursor = more.next
        this.changed()
      }
      this.page = clampPage(this.page, this.items.items.length, size, this.hasMore)
      this.error = null
    } catch (e) {
      if (request !== this.#request) return
      this.error = errorText(e)
      this.page = clampPage(this.page, this.items.items.length, size)
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

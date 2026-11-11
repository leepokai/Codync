import { useEffect, useState } from 'react'
import type { BotCatalog } from '@shared/catalog'
import type { BotStore } from '../../store/bot-store'
import { errorText } from './drafts'

export interface CatalogState {
  catalog: BotCatalog | null
  failure: string | null
}

/** The selected computer's ready-made bots and teams; an older host without a catalog fails quietly to empty. */
export function useCatalog(store: BotStore): CatalogState {
  const [state, setState] = useState<CatalogState & { owner: BotStore | null; context: string; connection: unknown }>({ catalog: null, failure: null, owner: null, context: '', connection: null })
  const online = store.connection.kind === 'online'
  const context = JSON.stringify([store.contextId, store.computer.id])
  const connection = store.connection
  useEffect(() => {
    if (!online) return
    let live = true
    store.botCatalog().then(
      (catalog) => live && setState({ catalog, failure: null, owner: store, context, connection }),
      (e: unknown) => live && setState({ catalog: null, failure: errorText(e), owner: store, context, connection }),
    )
    return () => {
      live = false
    }
  }, [store, online, context, connection])
  return online && state.owner === store && state.context === context && state.connection === connection
    ? state : { catalog: null, failure: null }
}

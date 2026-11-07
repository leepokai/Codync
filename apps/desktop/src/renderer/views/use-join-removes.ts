import { useEffect, useState } from 'react'
import type { BotStore } from '../store/bot-store'
import { useApp } from '../store/context'

/** What `load` answers about `store`, while `open`; `undefined` until it has. */
function useNames(load: (store: BotStore) => Promise<string[] | null>, store: BotStore | null, open: boolean) {
  const [names, setNames] = useState<string[] | null>()
  useEffect(() => {
    setNames(undefined)
    if (!open || !store) return
    let current = true
    void load(store).then((found) => current && setNames(found))
    return () => {
      current = false
    }
  }, [load, store, open])
  return names
}

/** The names of the phones `store`'s computer would remove by joining the signed-in account, while `open`; `null`: it couldn't say. */
export function useJoinRemoves(store: BotStore | null, open: boolean) {
  const { cloud } = useApp()
  return useNames(cloud.joinWouldRemove, store, open)
}

/** The names of the devices `store`'s computer would drop by leaving its account, while `open`. */
export function useLeaveRemoves(store: BotStore | null, open: boolean) {
  const { cloud } = useApp()
  return useNames(cloud.deviceNames, store, open)
}

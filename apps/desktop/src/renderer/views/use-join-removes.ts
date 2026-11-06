import { useEffect, useState } from 'react'
import type { BotStore } from '../store/bot-store'
import { useApp } from '../store/context'

/** The names of the phones `store`'s computer would remove by joining the signed-in account, while `open`. */
export function useJoinRemoves(store: BotStore | null, open: boolean) {
  const app = useApp()
  const [names, setNames] = useState<string[]>([])
  useEffect(() => {
    if (!open || !store) return setNames([])
    let current = true
    void app.cloud.joinWouldRemove(store).then((removed) => current && setNames(removed))
    return () => {
      current = false
    }
  }, [app, store, open])
  return names
}

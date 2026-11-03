import type { CloudComputer } from '../store/cloud'

/**
 * Which account computers to open through the relay: approved ones (`access` `granted`) other than this computer
 * and any the app reaches otherwise (SSH). Of the relay stores `attached` earlier, `kept` stay; every other one is
 * let go and its channel closed (`close`), and those whose grant went also lose their store (`detach`). One another
 * route took over already has its own store.
 */
export function relayPlan(computers: CloudComputer[], localId: string | null, attached: ReadonlySet<string>, store: (id: string) => { isLoopback: boolean } | null) {
  const granted = new Map(computers.filter((c) => c.access === 'granted' && c.computerId !== localId).map((c) => [c.computerId, c]))
  const relayed = (id: string) => store(id)?.isLoopback === false
  const kept = new Set([...attached].filter((id) => granted.has(id) && relayed(id)))
  const close = [...attached].filter((id) => !kept.has(id))
  return {
    attach: [...granted.values()].filter((c) => !store(c.computerId)),
    detach: close.filter(relayed),
    close,
    kept,
  }
}

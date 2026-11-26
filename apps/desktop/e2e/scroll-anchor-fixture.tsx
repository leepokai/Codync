// Mounted inside the running Electron sandbox by scroll-anchor-fixture.spec.ts. It renders the real ChatLog with
// controlled rows, so the list mutations a live host cannot seed (a deleted row, a trace-only cache) are measured in
// the actual renderer and DOM layout rather than inferred from window arithmetic.
import { useState } from 'react'
import type { Entry } from '../src/shared/models'
import type { ChatItem } from '../src/renderer/views/chat/chat-items'
import { ChatLog } from '../src/renderer/views/chat/ChatLog'
import { mountOverlay } from './overlay'

const ROW_HEIGHT = 80

/** A plain visible row with a fixed height, so a pixel move is unambiguous. */
const entry = (id: string, at: number): ChatItem => {
  const value: Entry = {
    id,
    seq: at + 1,
    botId: 'fixture-bot',
    rev: 1,
    kind: 'user',
    turn: 0,
    data: { text: `row ${id}` },
    createdAt: 1_000 + at,
    updatedAt: 1_000 + at,
  }
  return { id, kind: 'entry', entry: value, groupStart: true }
}

const rows = (count: number) => Array.from({ length: count }, (_, i) => entry(`e${i}`, i))

export type LoadMode = 'none' | 'one-visible' | 'hold'

export function mount({ count = 80, canLoadOlder = false, loadMode = 'none' }: {
  count?: number
  canLoadOlder?: boolean
  loadMode?: LoadMode
} = {}) {
  let current = rows(count)
  let setList: ((next: ChatItem[]) => void) | null = null
  let setReveal: ((next: { entryId: string } | null) => void) | null = null
  const calls: string[] = []

  function Fixture() {
    const [items, update] = useState<ChatItem[]>(current)
    const [revealed, updateReveal] = useState<{ entryId: string } | null>(null)
    setList = (next) => {
      current = next
      update(next)
    }
    setReveal = updateReveal
    return (
      <div style={{ position: 'relative', width: '900px', height: '600px' }}>
        <ChatLog
          items={items}
          fresh={() => current}
          canLoadOlder={canLoadOlder}
          loadOlder={async () => {
            calls.push('loadOlder')
            if (loadMode === 'hold') return new Promise(() => {})
            if (loadMode === 'one-visible') setList!([entry('older', -1), ...current])
          }}
          revealed={revealed}
          row={(item) => (
            <div style={{ height: ROW_HEIGHT, boxSizing: 'border-box', padding: 16, borderBottom: '1px solid #ddd' }}>
              {item.kind === 'separator' ? 'separator' : item.kind === 'entry' ? item.entry.id : item.kind}
            </div>
          )}
        />
      </div>
    )
  }

  const unmount = mountOverlay('e2e-scroll-anchor', <Fixture />)
  return {
    unmount,
    calls: () => [...calls],
    remove: (id: string) => setList?.(current.filter((item) => item.id !== id)),
    removeMany: (ids: string[]) => {
      const gone = new Set(ids)
      setList?.(current.filter((item) => !gone.has(item.id)))
    },
    regroup: (first: string, second: string) => {
      const at = current.findIndex((item) => item.id === first)
      const one = current[at]
      const two = current.find((item) => item.id === second)
      if (!one || one.kind !== 'entry' || !two || two.kind !== 'entry') return
      const grouped: ChatItem = { id: `group-${first}`, kind: 'requests', entry: one.entry, entries: [one.entry, two.entry] }
      setList?.([...current.slice(0, at), grouped, ...current.slice(at + 2)])
    },
    append: (id: string) => setList?.([...current, entry(id, current.length)]),
    reveal: (entryId: string) => setReveal?.({ entryId }),
  }
}

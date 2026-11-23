import { useEffect, useState } from 'react'
import { CardForm, CardSection, SearchField } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader, SheetPage } from '../../components/Overlay'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { type AgentGearItem } from '../../store/bot-store'
import { FormSheet } from './pieces'
import './marketplace.css'

const ICONS = { Skills: 'book.pages', Connectors: 'point.3.connected.trianglepath.dotted', Rules: 'list.bullet.rectangle', Plugins: 'square.grid.2x2' } as const

type Gear = { skills: AgentGearItem[]; connectors: AgentGearItem[]; rules: AgentGearItem[]; plugins: AgentGearItem[] }

/**
 * Skills, connectors, rules and plugins the agent already has in the user's home, which every login of it brings (an account profile's
 * folder shares that setup): on Login profiles under the agent's logins, or on its sheet for an agent without login profiles.
 */
export function AgentBrings({ backendId, title = 'What the agent brings' }: { backendId: string; title?: string }) {
  const store = useStore()
  const tr = useTr()
  const [gear, setGear] = useState<Gear | null>(null)
  const [failed, setFailed] = useState(false)
  const [open, setOpen] = useState<string | null>(null)
  const [query, setQuery] = useState('')
  // Loaded again when the computer comes back online, so a failed load doesn't stay.
  const online = store.connection.kind === 'online'

  useEffect(() => {
    if (!online) return
    let cancel = false
    setGear(null)
    setFailed(false)
    store.agentGear(backendId).then((loaded) => {
      if (!cancel) setGear(loaded)
    }).catch(() => {
      if (!cancel) setFailed(true)
    })
    return () => { cancel = true }
  }, [store, backendId, online])

  if (failed) {
    return <CardSection><Text size="footnote" tone="danger">{tr("Couldn't load what this agent brings.")}</Text></CardSection>
  }
  if (!gear) return null
  const rows = [
    ['Skills', gear.skills],
    ['Connectors', gear.connectors],
    ['Rules', gear.rules],
    ['Plugins', gear.plugins],
  ] as const
  if (rows.every(([, items]) => items.length === 0)) return null
  const shown = rows.find(([kind]) => kind === open)
  const needle = query.trim().toLowerCase()
  const matches = (shown?.[1] ?? []).filter((item) => !needle || [item.name, item.description, item.detail].some((t) => t?.toLowerCase().includes(needle)))
  const show = (kind: string) => {
    setQuery('')
    setOpen(kind)
  }
  return (
    <>
      <CardSection title={title}>
        {rows.filter(([, items]) => items.length > 0).map(([kind, items]) => (
          <button key={kind} className="agent-brings-row" onClick={() => show(kind)}>
            <span className="agent-brings-icon"><Icon name={ICONS[kind]} size={13} /></span>
            <Text>{tr(kind)}</Text>
            <span className="agent-brings-count">{items.length}</span>
            <Icon name="chevron.right" size={10} weight="semibold" color="var(--tertiary)" />
          </button>
        ))}
      </CardSection>
      {/* One kind's items in a modal of their own: a list of 100 skills doesn't push the page apart. */}
      <FormSheet open={shown !== undefined} onClose={() => setOpen(null)} width={520} height={600}>
        {shown ? (
          <SheetPage>
            <ModalHeader icon={ICONS[shown[0]]} title={tr(shown[0])} subtitle={String(shown[1].length)} onClose={() => setOpen(null)} />
            <div className="agent-brings-search"><SearchField value={query} onChange={setQuery} /></div>
            <CardForm>
              {matches.length === 0 ? <Text tone="secondary">{tr('No matches')}</Text> : null}
              <CardSection>
                {matches.map((item) => {
                  const note = item.description || item.detail
                  return (
                    <VStack key={item.name} gap={2} className="agent-brings-item">
                      <Text size="subheadline" weight="semibold" truncate>{item.name}</Text>
                      {note ? <Text size="caption" tone="secondary" lines={2}>{note}</Text> : null}
                    </VStack>
                  )
                })}
              </CardSection>
            </CardForm>
          </SheetPage>
        ) : null}
      </FormSheet>
    </>
  )
}

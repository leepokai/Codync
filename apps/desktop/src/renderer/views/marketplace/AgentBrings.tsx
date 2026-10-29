import { useEffect, useState } from 'react'
import { CardSection, Disclosure } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { type AgentGearItem } from '../../store/bot-store'
import './marketplace.css'

const ICONS = { Skills: 'book.pages', Connectors: 'point.3.connected.trianglepath.dotted', Rules: 'list.bullet.rectangle', Plugins: 'square.grid.2x2' } as const

type Gear = { skills: AgentGearItem[]; connectors: AgentGearItem[]; rules: AgentGearItem[]; plugins: AgentGearItem[] }

/** Skills, connectors, rules and plugins the agent already has, under Sign in on its sheet. */
export function AgentBrings({ backendId }: { backendId: string }) {
  const store = useStore()
  const tr = useTr()
  const [gear, setGear] = useState<Gear | null>(null)
  const [failed, setFailed] = useState(false)
  const [open, setOpen] = useState<string | null>(null)
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
  return (
    <CardSection title="What the agent brings">
      {rows.filter(([, items]) => items.length > 0).map(([title, items]) => (
        <Disclosure
          key={title}
          expanded={open === title}
          onToggle={() => setOpen(open === title ? null : title)}
          label={(
            <HStack as="span" className="agent-brings-row" gap={10}>
              <span className="agent-brings-icon"><Icon name={ICONS[title]} size={13} /></span>
              <Text>{tr(title)}</Text>
              <span className="agent-brings-count">{items.length}</span>
            </HStack>
          )}
        >
          <div className="agent-brings-items">
            {items.map((item) => {
              const note = item.description || item.detail
              return (
                <div className="agent-brings-item" key={item.name}>
                  <Text size="subheadline" weight="semibold" truncate>{item.name}</Text>
                  {note ? <Text size="caption" tone="secondary" lines={2}>{note}</Text> : null}
                </div>
              )
            })}
          </div>
        </Disclosure>
      ))}
    </CardSection>
  )
}

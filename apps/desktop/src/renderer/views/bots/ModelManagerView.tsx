import { useState } from 'react'
import { draftOf, isGroup } from '@shared/models'
import { AgentIcon } from '../../components/AgentIcon'
import { AvatarWithStatus } from '../../components/Avatar'
import { Button, IconButton } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Sheet } from '../../components/Overlay'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { refKey, type RosterItem } from '../../store/app-model'
import { analytics } from '../../store/analytics'
import { StoreContext, useStore } from '../../store/context'
import { matchBots } from '../SearchPalette'
import { AgentModelPicker } from './AgentModelPicker'
import { useAgentModels, type AgentModels } from './agent-models'
import { errorText, normalized } from './drafts'
import { agentChoice } from './Fallbacks'
import { allKeys, filterChecked, planRows, toggleKey, type ApplyPlan } from './model-manager-logic'
import '../search-palette.css'
import './model-manager.css'

type Pick = { backend: string; model: string | null; modelOptions: Record<string, string> }
type Outcome = { ok: boolean; text: string }

/** What the bot is for, plus its computer when the roster spans more than one. */
const subtitle = ({ bot, store, computer }: RosterItem, oneComputer: boolean) =>
  [bot.description, oneComputer ? '' : (store.hostName ?? computer.name)].filter(Boolean).join(' ? ')

/** Apply one agent, model and its model options to many bots at once. */
export function ModelManagerView({ items, onClose }: { items: RosterItem[]; onClose: () => void }) {
  const tr = useTr()
  const store = items[0]?.store
  return (
    <Sheet open onClose={onClose} width={640}>
      <VStack gap={12} className="model-manager">
        <HStack gap={8} className="model-manager-head">
          <Text size="body" tone="text" className="model-manager-title">{tr('Model Manager')}</Text>
          <Spacer />
          <IconButton title={tr('Close')} icon="xmark" size={28} onClick={onClose} />
        </HStack>
        {store ? (
          <StoreContext.Provider value={store}>
            <ManagerBody items={items} />
          </StoreContext.Provider>
        ) : (
          <Text size="caption" tone="secondary">{tr('No bots selected')}</Text>
        )}
      </VStack>
    </Sheet>
  )
}

function ManagerBody({ items }: { items: RosterItem[] }) {
  const tr = useTr()
  const store = useStore()
  const first = items[0]
  const [pick, setPick] = useState<Pick>({ backend: first?.bot.backend ?? 'custom', model: null, modelOptions: {} })
  const [query, setQuery] = useState('')
  const [checked, setChecked] = useState<Set<string>>(() => new Set())
  const [outcomes, setOutcomes] = useState<Record<string, Outcome>>({})
  const [applying, setApplying] = useState(false)
  const models = useAgentModels(pick.backend)
  const backends = store.hello?.backends ?? []
  const agents = [...backends.map(agentChoice), { id: 'custom', label: 'Custom command', leading: <AgentIcon registry="custom" size={14} /> }]
  const shown = matchBots(items, query)
  const keys = allKeys(items)
  const everyChecked = keys.length > 0 && keys.every((key) => checked.has(key))
  const oneComputer = !items.some((item) => item.computer.id !== first?.computer.id)
  const canApply = checked.size > 0 && !applying && (pick.backend === 'custom' || models.catalog !== null)

  const toggle = (key: string, on: boolean) => setChecked((set) => toggleKey(set, key, on))
  const markAll = (on: boolean) => setChecked((set) => keys.reduce((next, key) => toggleKey(next, key, on), set))

  const apply = async () => {
    const chosen = filterChecked(items, checked)
    if (!chosen.length || applying) return
    setApplying(true)
    setOutcomes({})
    const catalogs: Record<string, AgentModels | null> = {}
    try {
      if (pick.model) {
        for (const item of chosen) {
          const id = item.computer.id
          if (id === store.computer.id || id in catalogs) continue
          catalogs[id] = await item.store
            .ready()
            .then((client) => client.call<AgentModels>('agentModels', { backend: pick.backend }))
            .catch(() => null)
        }
      }
      const plan: ApplyPlan = { items: chosen, backend: pick.backend, model: pick.model, options: pick.modelOptions }
      const rows = planRows(plan, (id) => (id === store.computer.id ? models.catalog : catalogs[id] ?? null))
      let applied = 0
      for (const row of rows) {
        const key = refKey(row.item.ref)
        if (!row.ok) {
          setOutcomes((o) => ({ ...o, [key]: { ok: false, text: row.reason ?? tr('Failed') } }))
          continue
        }
        const draft = normalized({ ...draftOf(row.item.bot), backend: pick.backend, model: pick.model || null, modelOptions: pick.modelOptions })
        try {
          await row.item.store.save(draft)
          applied++
          setOutcomes((o) => ({ ...o, [key]: { ok: true, text: tr('Applied') } }))
        } catch (e) {
          setOutcomes((o) => ({ ...o, [key]: { ok: false, text: errorText(e) } }))
        }
      }
      analytics.track({ event: 'model_manager_applied', requested: chosen.length, applied, failed: chosen.length - applied })
    } finally {
      setApplying(false)
    }
  }

  return (
    <VStack gap={12} className="model-manager-body">
      <AgentModelPicker
        backend={pick.backend}
        models={models}
        selection={pick.model}
        onChange={(model, modelOptions) => setPick((p) => ({ ...p, model: model || null, modelOptions }))}
        options={pick.modelOptions}
        agents={agents}
        onAgentChange={(id) => setPick({ backend: id, model: null, modelOptions: {} })}
      />
      <div className="palette-field">
        <Icon name="magnifyingglass" size={15} color="var(--secondary)" />
        <input value={query} placeholder={tr('Search bots')} aria-label={tr('Search bots')} spellCheck={false} onChange={(e) => setQuery(e.target.value)} />
      </div>
      <HStack gap={8} className="model-manager-tools">
        <Button onClick={() => markAll(true)} disabled={applying || everyChecked}>{tr('Select all')}</Button>
        <Button onClick={() => markAll(false)} disabled={applying || checked.size === 0}>{tr('Unselect all')}</Button>
        <Spacer />
        {checked.size === 0 ? <Text size="caption" tone="secondary">{tr('No bots selected')}</Text> : null}
        <Button onClick={apply} disabled={!canApply}>{tr('Apply')}</Button>
      </HStack>
      <div className="palette-list model-manager-list">
        {shown.map((item) => {
          const key = refKey(item.ref)
          const on = checked.has(key)
          const outcome = outcomes[key]
          const line = subtitle(item, oneComputer)
          return (
            <button key={key} type="button" role="checkbox" aria-checked={on} className="palette-row model-manager-row" disabled={applying} onClick={() => toggle(key, !on)}>
              <span className={`model-manager-check ${on ? 'on' : ''}`} aria-hidden="true">{on ? <Icon name="checkmark" size={11} /> : null}</span>
              <AvatarWithStatus bot={item.bot} members={isGroup(item.bot) ? item.store.members(item.bot) : []} size={32} />
              <VStack as="span" gap={2} grow>
                <Text size="body" tone="text" truncate>{item.bot.name}</Text>
                {line ? <Text size="subheadline" tone="secondary" truncate>{line}</Text> : null}
              </VStack>
              {outcome ? (
                <span className={`model-manager-status ${outcome.ok ? '' : 'bad'}`}>
                  <Icon name={outcome.ok ? 'checkmark' : 'exclamationmark.triangle'} size={12} />
                  <Text size="caption" tone={outcome.ok ? 'secondary' : 'attention'} truncate>{outcome.text}</Text>
                </span>
              ) : null}
            </button>
          )
        })}
        {shown.length === 0 ? <div className="palette-empty">{tr('No matching bots')}</div> : null}
      </div>
    </VStack>
  )
}

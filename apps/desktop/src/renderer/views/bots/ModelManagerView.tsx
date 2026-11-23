import { useEffect, useRef, useState } from 'react'
import { isGroup } from '@shared/models'
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
import { errorText } from './drafts'
import { agentChoice } from './Fallbacks'
import { allKeys, filterChecked, planRows, toggleKey, type ApplyPlan, type CatalogCheck } from './model-manager-logic'
import '../search-palette.css'
import './model-manager.css'

type Pick = { backend: string; model: string | null; modelOptions: Record<string, string> }
type Outcome = { ok: boolean; text: string }

/** What the bot is for, plus its computer when the roster spans more than one. */
const subtitle = ({ bot, computer }: RosterItem, oneComputer: boolean) =>
  [bot.description, oneComputer ? '' : computer.name].filter(Boolean).join(' · ')

/** Apply one agent, model and its model options to many bots at once. */
export function ModelManagerView({ items, onClose }: { items: RosterItem[]; onClose: () => void }) {
  const tr = useTr()
  // Groups carry members, not a model of their own; the batch swap targets bots.
  const bots = items.filter((item) => !isGroup(item.bot))
  const store = bots[0]?.store
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
            <ManagerBody items={bots} />
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
  const backends = store.hello?.backends ?? []
  const initialBackend = typeof first?.bot.backend === 'string' && first.bot.backend !== 'custom' ? first.bot.backend : String(backends[0] ?? 'claude')
  const [pick, setPick] = useState<Pick>({ backend: initialBackend, model: null, modelOptions: {} })
  const [query, setQuery] = useState('')
  const [checked, setChecked] = useState<Set<string>>(() => new Set())
  const [outcomes, setOutcomes] = useState<Record<string, Outcome>>({})
  const [applying, setApplying] = useState(false)
  const busy = useRef(false)
  const alive = useRef(true)
  const run = useRef(0)
  useEffect(() => () => { alive.current = false }, [])
  const models = useAgentModels(pick.backend)
  // A custom command needs the editor's command field, which a batch apply can't supply; the batch
  // swap targets the agents the account's computers offer.
  const agents = backends.map(agentChoice)
  const shown = matchBots(items, query)
  const shownKeys = allKeys(shown)
  const everyChecked = shownKeys.length > 0 && shownKeys.every((key) => checked.has(key))
  const oneComputer = !items.some((item) => item.computer.id !== first?.computer.id)
  const canApply = checked.size > 0 && !applying

  const toggle = (key: string, on: boolean) => setChecked((set) => toggleKey(set, key, on))
  const markAll = (on: boolean) => setChecked((set) => shownKeys.reduce((next, key) => toggleKey(next, key, on), set))

  const apply = async () => {
    const chosen = filterChecked(items, checked)
    if (!chosen.length || busy.current) return
    const myRun = run.current
    const backend = pick.backend
    const model = pick.model?.trim() || null
    const options = { ...pick.modelOptions }
    busy.current = true
    setApplying(true)
    setOutcomes({})
    let applied = 0
    let failed = 0
    const catalogs: Record<string, CatalogCheck> = {}
    try {
      if (model) {
        for (const item of chosen) {
          if (!alive.current || run.current !== myRun) return
          const id = item.ref.computerId
          if (id === store.computer.id || id in catalogs) continue
          catalogs[id] = await item.store
            .ready()
            .then((client) => client.call<AgentModels>('agentModels', { backend }, 300_000))
            .then((catalog): CatalogCheck => ({ kind: 'ok', catalog }))
            .catch((): CatalogCheck => ({ kind: 'error' }))
        }
      }
      const plan: ApplyPlan = { items: chosen, backend, model, options }
      const rows = planRows(plan, (id) => {
        if (id === store.computer.id) return models.failure ? { kind: 'error' } : models.catalog ? { kind: 'ok', catalog: models.catalog } : { kind: 'none' }
        return catalogs[id] ?? { kind: 'none' }
      })
      for (const row of rows) {
        if (!alive.current || run.current !== myRun) return
        const key = refKey(row.item.ref)
        if (!row.ok) {
          failed++
          setOutcomes((o) => ({ ...o, [key]: { ok: false, text: row.reason ?? tr('Failed') } }))
          continue
        }
        try {
          // A minimal patch: the host merges it over the bot's state as it is at that moment, under
          // one lock, so an unrelated change that landed mid-batch is never written back. On another
          // agent the host moves the bot to that agent's Auto mode on its own and drops a login
          // profile that doesn't fit it; the custom command is cleared here, as the bot editor's
          // agent change does. Fallbacks are not touched.
          const patch: Record<string, unknown> = { id: row.item.ref.botId, backend, model: model || null, modelOptions: { ...options } }
          if (backend !== 'custom') patch.command = null
          await row.item.store
            .ready()
            .then((client) => client.call('updateBot', patch))
            .then(() => {
              applied++
              if (alive.current && run.current === myRun) setOutcomes((o) => ({ ...o, [key]: { ok: true, text: tr('Applied') } }))
            })
        } catch (e) {
          failed++
          if (alive.current && run.current === myRun) setOutcomes((o) => ({ ...o, [key]: { ok: false, text: errorText(e) } }))
        }
      }
    } finally {
      // One event per apply, whatever ended it: closed mid-run, pick changed mid-run, or completed.
      // Rows the run never reached are the remainder (requested - applied - failed).
      analytics.track({ event: 'model_manager_applied', requested: chosen.length, applied, failed })
      busy.current = false
      setApplying(false)
    }
  }

  return (
    <VStack gap={12} className="model-manager-body">
      <AgentModelPicker
        backend={pick.backend}
        models={models}
        selection={pick.model}
        onChange={(model, modelOptions) => { run.current++; setOutcomes({}); setPick((p) => ({ ...p, model: model || null, modelOptions })) }}
        options={pick.modelOptions}
        agents={agents}
        onAgentChange={(id) => { run.current++; setOutcomes({}); setPick({ backend: id, model: null, modelOptions: {} }) }}
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

import { useEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type PointerEvent as ReactPointerEvent } from 'react'
import { isGroup } from '@shared/models'
import { AgentIcon, agentTint } from '../../components/AgentIcon'
import { AvatarWithStatus } from '../../components/Avatar'
import { Button, DropdownMenu, IconButton, SearchField } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import type { MenuItem } from '../../components/Menu'
import { Sheet } from '../../components/Overlay'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { refKey, type RosterItem } from '../../store/app-model'
import { analytics } from '../../store/analytics'
import { StoreContext, useStore } from '../../store/context'
import { botWords } from '../search-items'
import { matchWords } from '../palette-items'
import { ModelPicker } from './ModelPicker'
import { useAgentModels, type AgentModels } from './agent-models'
import { errorText } from './drafts'
import { loginSeats } from './seats'
import { allKeys, applyTargets, clampSheetSize, defaultSheetSize, pausePatch, providerOptions, resizeFrom, rowModelPath, SHEET_MIN, stepSize, planRows, toggleKey, type ApplyPlan, type CatalogCheck } from './model-manager-logic'
import '../search-palette.css'
import './model-manager.css'

type Selection = { backend: string; model: string | null; modelOptions: Record<string, string> }
type Outcome = { ok: boolean; text: string }

/** What the bot is for, plus its computer when the roster spans more than one. */
const subtitle = ({ bot, computer }: RosterItem, oneComputer: boolean) =>
  [bot.description, oneComputer ? '' : computer.name].filter(Boolean).join(' · ')

/** Measures rendered text with the app's font, so the sheet can size itself to its content. */
const makeMeasurer = () => {
  const context = document.createElement('canvas').getContext('2d')
  if (!context) return (text: string) => text.length * 7
  return (text: string) => {
    // The rows render at the body size (13px) times the user's text scale, weight 400.
    const root = getComputedStyle(document.documentElement)
    const scale = parseFloat(root.getPropertyValue('--scale')) || 1
    context.font = `400 ${13 * scale}px ${root.getPropertyValue('--font') || 'system-ui'}`
    return context.measureText(text).width
  }
}

/** Apply one agent, model and its model options to many bots at once. */
export function ModelManagerView({ items, onClose }: { items: RosterItem[]; onClose: () => void }) {
  const tr = useTr()
  // Groups carry members, not a model of their own; the batch swap targets bots.
  const bots = items.filter((item) => !isGroup(item.bot))
  const store = bots[0]?.store
  // The sheet opens big (about three quarters of the main window's height) and sized to the
  // longest bot name, and stays draggable afterwards; the main window shrinking clamps it back.
  const measure = useMemo(makeMeasurer, [])
  const [size, setSize] = useState(() =>
    defaultSheetSize(bots.map((item) => item.bot.name), measure, window.innerWidth, window.innerHeight))
  useEffect(() => {
    const onResize = () => setSize((s) => clampSheetSize(s.width, s.height, window.innerWidth, window.innerHeight))
    window.addEventListener('resize', onResize)
    return () => window.removeEventListener('resize', onResize)
  }, [])
  return (
    <Sheet open onClose={onClose} width={size.width} height={size.height}>
      <VStack gap={12} className="model-manager">
        <HStack gap={8} className="model-manager-head">
          <Text size="body" tone="text" className="model-manager-title">{tr('Bot Manager')}</Text>
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
        {/* The drag handles render last: absolute positioning frees them from DOM order, so Tab
            reaches Close and every control before any resize handle. */}
        <SheetHandle kind="left" label={tr('Resize width')} size={size} onSize={setSize} />
        <SheetHandle kind="right" label={tr('Resize width')} size={size} onSize={setSize} />
        <SheetHandle kind="bottom" label={tr('Resize height')} size={size} onSize={setSize} />
        <SheetHandle kind="corner-br" label="" size={size} onSize={setSize} />
        <SheetHandle kind="corner-bl" label="" size={size} onSize={setSize} />
      </VStack>
    </Sheet>
  )
}

/**
 * One draggable edge or corner of the sheet: a pointer drag (captured, so it keeps working over
 * the content) with the arrows stepping it like the app's panel ResizeHandle — but anchored to
 * the drag's start size, never the live one, so cumulative pointer deltas can't compound. The
 * corners are pointer-only duplicates of the edges and carry no roles.
 */
function SheetHandle({ kind, label, size, onSize }: {
  kind: 'left' | 'right' | 'bottom' | 'corner-br' | 'corner-bl'
  label: string
  size: { width: number; height: number }
  onSize: (size: { width: number; height: number }) => void
}) {
  const start = useRef<{ x: number; y: number; width: number; height: number } | null>(null)
  const resizingClass = kind === 'left' || kind === 'right' ? 'mm-resizing-ew'
    : kind === 'bottom' ? 'mm-resizing-ns'
    : kind === 'corner-br' ? 'mm-resizing-nwse' : 'mm-resizing-nesw'
  const corner = kind === 'corner-br' || kind === 'corner-bl'
  const end = () => {
    if (!start.current) return
    start.current = null
    document.documentElement.classList.remove(resizingClass)
  }
  useEffect(() => () => { document.documentElement.classList.remove(resizingClass) }, [])
  const onPointerDown = (e: ReactPointerEvent) => {
    if (e.button !== 0) return
    e.currentTarget.setPointerCapture(e.pointerId)
    start.current = { x: e.clientX, y: e.clientY, width: size.width, height: size.height }
    document.documentElement.classList.add(resizingClass)
  }
  const onPointerMove = (e: ReactPointerEvent) => {
    // The deltas are cumulative since pointerdown, so the anchor is the drag's START size — the
    // live one is already updated by earlier moves and would compound every event.
    if (!start.current) return
    onSize(resizeFrom(kind, start.current, e.clientX - start.current.x, e.clientY - start.current.y, window.innerWidth, window.innerHeight))
  }
  const onKeyDown = (e: ReactKeyboardEvent) => {
    // Tab/Shift+Tab must keep moving focus (the ResizeHandle precedent gates on arrows only);
    // swallowing them here would trap a keyboard user on a handle with Escape as the only exit.
    if (!e.key.startsWith('Arrow')) return
    e.preventDefault()
    onSize(stepSize(kind, size, e.key, window.innerWidth, window.innerHeight))
  }
  const common = {
    onPointerDown,
    onPointerMove,
    onPointerUp: end,
    onPointerCancel: end,
    onLostPointerCapture: end,
  }
  return corner ? (
    <div className={`model-manager-resize mm-${kind}`} aria-hidden="true" {...common} />
  ) : (
    <div
      className={`model-manager-resize mm-${kind}`}
      role="separator"
      aria-label={label}
      aria-orientation={kind === 'bottom' ? 'horizontal' : 'vertical'}
      aria-valuenow={kind === 'bottom' ? Math.round(size.height) : Math.round(size.width)}
      aria-valuemin={kind === 'bottom' ? SHEET_MIN.height : SHEET_MIN.width}
      tabIndex={0}
      onKeyDown={onKeyDown}
      {...common}
    />
  )
}

function ManagerBody({ items }: { items: RosterItem[] }) {
  const tr = useTr()
  const store = useStore()
  const first = items[0]
  const backends = store.hello?.backends ?? []
  const initialBackend = typeof first?.bot.backend === 'string' && first.bot.backend !== 'custom'
    ? first.bot.backend
    : (backends.find((b) => b.id !== 'custom')?.id ?? 'claude')
  const [pick, setPick] = useState<Selection>({ backend: initialBackend, model: null, modelOptions: {} })
  const [query, setQuery] = useState('')
  const searchField = useRef<HTMLInputElement>(null)
  // The agent filter, composed with the search over the ONE shared matcher: `''` is every provider.
  const [provider, setProvider] = useState('')
  const [checked, setChecked] = useState<Set<string>>(() => new Set())
  const [outcomes, setOutcomes] = useState<Record<string, Outcome>>({})
  const [applying, setApplying] = useState(false)
  const busy = useRef(false)
  const alive = useRef(true)
  const run = useRef(0)
  useEffect(() => () => { alive.current = false }, [])
  const models = useAgentModels(pick.backend)
  // A custom command needs the editor's command field, which a batch apply can't supply; the batch
  // swap targets the agents the account's computers offer. The sheet rides the same seat rail the
  // bot editor's model picker shows, but one seat per agent — its default login — because a batch
  // writes no login profile (each bot keeps its own), so that is exactly what it can target.
  const seats = loginSeats({ backends, on: { backend: pick.backend, loginProfile: null }, logins: () => [null], tr, pick: (b) => { run.current++; setOutcomes({}); setPick({ backend: b.id, model: null, modelOptions: {} }) } })
  // The agents the bots actually run — derived live from the roster, never a hardcoded list — and
  // the one the filter is narrowed to. A provider whose bots all left is forgotten, so it cannot
  // silently re-narrow the list when one of them returns; the effect keys on the joined ids so a
  // new array each render doesn't re-run it.
  const providers = providerOptions(items, backends)
  const chosenProvider = providers.find((p) => p.id === provider) ?? null
  const providerIds = providers.map((p) => p.id).join('\0')
  useEffect(() => {
    if (provider && !providerIds.split('\0').includes(provider)) setProvider('')
  }, [provider, providerIds])
  const shown = matchWords(items.map((item) => ({ item, words: botWords(item) })), query)
    .map((w) => w.item)
    .filter((item) => !chosenProvider || item.bot.backend === chosenProvider.id)
  const shownKeys = allKeys(shown)
  const everyChecked = shownKeys.length > 0 && shownKeys.every((key) => checked.has(key))
  const oneComputer = !items.some((item) => item.computer.id !== first?.computer.id)
  // Apply only ever touches what the user can see: checked bots the filter hides stay checked
  // and untouched, and the counts below say so.
  const targets = applyTargets(items, shown, checked)
  const canApply = targets.chosen.length > 0 && !applying

  const toggle = (key: string, on: boolean) => setChecked((set) => toggleKey(set, key, on))
  // "Select all" checks the filtered list; "Unselect all" clears the whole selection, hidden
  // bots included — the label promises all, and the counts below keep it honest.
  const markAll = (on: boolean) => {
    if (on) setChecked((set) => shownKeys.reduce((next, key) => toggleKey(next, key, on), set))
    else setChecked(new Set())
  }

  // Pause or resume every checked bot the filter still shows, one minimal patch each — on the same
  // `updateBot` path the roster's own pause toggle uses, and more minimal: `{ id, paused }` only, so
  // the host keeps every other setting and its pause rules (answers nothing, spends nothing) apply
  // exactly as they do from the roster.
  // The sheet acts as the user, like Apply; a bot pausing another goes through the manage tool's
  // own gate. Mid-batch, the host's own merge keeps an unrelated change from being written back.
  const setPausedAll = async (paused: boolean) => {
    const chosen = targets.chosen
    if (!chosen.length || busy.current) return
    const myRun = run.current
    busy.current = true
    setApplying(true)
    setOutcomes({})
    let applied = 0
    let failed = 0
    try {
      for (const item of chosen) {
        if (!alive.current || run.current !== myRun) return
        const key = refKey(item.ref)
        try {
          await item.store
            .ready()
            .then((client) => {
              // The wait for a reconnect may outlast the run: re-check before anything is sent.
              if (!alive.current || run.current !== myRun) return 'skipped'
              return client.call('updateBot', pausePatch(item.ref.botId, paused)).then((): 'sent' | 'skipped' => 'sent')
            })
            .then((sent) => {
              if (sent !== 'sent') return
              applied++
              if (alive.current && run.current === myRun) {
                setOutcomes((o) => ({ ...o, [key]: { ok: true, text: tr(paused ? 'Paused' : 'Resumed') } }))
              }
            })
        } catch (e) {
          failed++
          if (alive.current && run.current === myRun) setOutcomes((o) => ({ ...o, [key]: { ok: false, text: errorText(e) } }))
        }
      }
    } finally {
      // One event per bulk pause or resume, like Apply's one per apply.
      analytics.track({ event: 'bot_manager_paused', requested: chosen.length, applied, failed, paused })
      busy.current = false
      setApplying(false)
    }
  }

  const apply = async () => {
    // Only the checked bots the filter still shows (pinned by applyTargets' tests): hidden
    // checked bots are left exactly as they are, so nothing the user cannot see is written.
    const chosen = targets.chosen
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
            .then(async (client): Promise<CatalogCheck> => {
              // The wait for a reconnect may outlast the run: re-check before anything is read.
              if (!alive.current || run.current !== myRun) return { kind: 'error' }
              const catalog = await client.call<AgentModels>('agentModels', { backend }, 300_000)
              return { kind: 'ok', catalog }
            })
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
          // agent change does. The patch sends no fallback list — the host's own fallback tidy may
          // still drop a step that duplicates the new primary agent, as on any agent change.
          const patch: Record<string, unknown> = { id: row.item.ref.botId, backend, model: model || null, modelOptions: { ...options } }
          if (backend !== 'custom') patch.command = null
          await row.item.store
            .ready()
            .then((client) => {
              // The wait for a reconnect may outlast the run: re-check before anything is sent.
              if (!alive.current || run.current !== myRun) return 'skipped'
              return client.call('updateBot', patch).then((): 'sent' | 'skipped' => 'sent')
            })
            .then((sent) => {
              if (sent !== 'sent') return
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
      <ModelPicker
        key="agent"
        trigger="row"
        backend={pick.backend}
        models={models}
        seats={seats}
        model={{ id: pick.model, options: pick.modelOptions }}
        set={(m) => { run.current++; setOutcomes({}); setPick((p) => ({ ...p, model: m.id || null, modelOptions: m.options })) }}
      >
        <Text size="caption" tone="secondary">{tr('Changing the model starts a new agent session. Chat history is kept.')}</Text>
      </ModelPicker>
      {/* The shared search field — the same one FindBar and the pickers use — so its clear ✕
          comes with it and the box is one implementation everywhere (the sync rule). */}
      <SearchField value={query} onChange={setQuery} prompt="Search bots" inputRef={searchField} />
      {providers.length > 1 ? (
        <DropdownMenu
          items={(): MenuItem[] => [
            { title: 'All providers', icon: 'square.grid.3x3', selected: !chosenProvider, action: () => setProvider('') },
            ...providers.map((p) => ({
              title: p.name,
              verbatim: true,
              leading: <AgentIcon registry={store.agentRegistry(p.id)} size={14} color={agentTint(store.agentRegistry(p.id))} />,
              selected: p.id === chosenProvider?.id,
              action: () => setProvider(p.id),
            })),
          ]}
          title={chosenProvider?.name ?? tr('All providers')}
          className={`model-manager-provider${chosenProvider ? ' on' : ''}`}
        >
          <Icon name="line.3.horizontal.decrease" size={13} color={chosenProvider ? 'var(--accent)' : 'var(--secondary)'} />
          <Text size="caption" tone={chosenProvider ? 'text' : 'secondary'}>{chosenProvider?.name ?? tr('All providers')}</Text>
        </DropdownMenu>
      ) : null}
      <HStack gap={8} className="model-manager-tools">
        <Button onClick={() => markAll(true)} disabled={applying || everyChecked || shownKeys.length === 0}>{tr('Select all')}</Button>
        <Button onClick={() => markAll(false)} disabled={applying || checked.size === 0}>{tr('Unselect all')}</Button>
        <Button onClick={() => void setPausedAll(true)} disabled={!canApply}>{tr('Pause checked')}</Button>
        <Button onClick={() => void setPausedAll(false)} disabled={!canApply}>{tr('Resume checked')}</Button>
        <Spacer />
        {targets.totalChecked === 0 ? <Text size="caption" tone="secondary">{tr('No bots selected')}</Text> : <Text size="caption" tone="secondary">{tr('Selected: {n}').replace('{n}', () => String(targets.totalChecked))}</Text>}
        <Button onClick={apply} disabled={!canApply}>{tr('Apply')}</Button>
      </HStack>
      {targets.hiddenCount > 0 ? (
        <Text size="caption" tone="attention">{tr('Bots hidden by the filter: {n} (no action touches them)').replace('{n}', () => String(targets.hiddenCount))}</Text>
      ) : null}
      <div className="palette-list model-manager-list">
        {shown.map((item) => {
          const key = refKey(item.ref)
          const on = checked.has(key)
          const outcome = outcomes[key]
          const line = subtitle(item, oneComputer)
          // The bot's current agent -> model path, under the name in the app's muted small text.
          const agentName = backends.find((b) => b.id === item.bot.backend)?.name ?? null
          const modelPath = rowModelPath(item.bot, agentName)
          return (
            <button key={key} type="button" role="checkbox" aria-checked={on} className="palette-row model-manager-row" disabled={applying} onClick={() => toggle(key, !on)}>
              <span className={`model-manager-check ${on ? 'on' : ''}`} aria-hidden="true">{on ? <Icon name="checkmark" size={11} /> : null}</span>
              <AvatarWithStatus bot={item.bot} members={isGroup(item.bot) ? item.store.members(item.bot) : []} size={32} />
              <VStack as="span" gap={2} grow>
                <Text size="body" tone="text" truncate>{item.bot.name}</Text>
                {item.bot.paused ? <Text size="caption" tone="attention" truncate>{tr('Paused')}</Text> : null}
                {modelPath ? <Text size="caption" tone="secondary" truncate className="model-manager-model-path">{modelPath}</Text> : null}
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

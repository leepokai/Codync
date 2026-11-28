import { useEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type PointerEvent as ReactPointerEvent } from 'react'
import type { AgentModels } from '@shared/agent-models'
import { isGroup } from '@shared/models'
import { AgentIcon, agentTint } from '../../components/AgentIcon'
import { AvatarWithStatus } from '../../components/Avatar'
import { Button, DropdownMenu, IconButton, Switch } from '../../components/Controls'
import type { MenuItem } from '../../components/Menu'
import { Icon } from '../../components/Icon'
import { Sheet } from '../../components/Overlay'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { refKey, type RosterItem } from '../../store/app-model'
import { analytics } from '../../store/analytics'
import { StoreContext, useStore } from '../../store/context'
import { matchItems } from '../palette-items'
import { botItems } from '../search-items'
import { ModelPicker } from './ModelPicker'
import { useAgentModels } from './agent-models'
import { agentModelsRequest } from './agent-models-query.ts'
import { pauseEcho } from '@shared/old-host'
import { errorText } from './drafts'
import { loginSeats } from './seats'
import { agentNameOf, allKeys, applyPatch, applyTargets, clampSheetSize, computerBackends, countApplied, countedPause, countFailure, defaultSheetSize, loginNameOf, loginProfileFor, modelPathWords, needsCatalog, pausePatch, providerOptions, remainingTargets, resizeFrom, rowModelPath, runTally, SHEET_MIN, stepSize, planRows, toggleKey, type ApplyPlan, type CatalogCheck } from './model-manager-logic'
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
  const initialFromBot = typeof first?.bot.backend === 'string' && first.bot.backend !== 'custom'
  const initialBackend = initialFromBot ? first.bot.backend : (backends.find((b) => b.id !== 'custom')?.id ?? 'claude')
  const [pick, setPick] = useState<Selection>({ backend: initialBackend, model: null, modelOptions: {} })
  const [query, setQuery] = useState('')
  // Whether the search also reads each bot's model path. On by default: a search for "deepseek"
  // is looking for the bots that run it. This switch narrows a search; it does not change what
  // the app does behind the user's back.
  const [modelSearch, setModelSearch] = useState(true)
  // The agent filter, composed with the search: `''` is every provider.
  const [provider, setProvider] = useState('')
  const [checked, setChecked] = useState<Set<string>>(() => new Set())
  const [outcomes, setOutcomes] = useState<Record<string, Outcome>>({})
  const [applying, setApplying] = useState(false)
  const busy = useRef(false)
  const alive = useRef(true)
  const run = useRef(0)
  useEffect(() => () => { alive.current = false }, [])
  const models = useAgentModels(pick.backend)
  // A custom command needs the editor's command field, which a batch apply can't supply, so it is not
  // offered. The rail is the one every picker shows (loginSeats), but one seat per agent — its default
  // login — because the model shown here is the account's own choice. Each bot is then checked and
  // saved on the login it already uses, which can differ between computers.
  const seats = loginSeats({
    backends,
    on: { backend: pick.backend, loginProfile: null },
    logins: () => [null],
    tr,
    pick: (b) => {
      // The editor's own rule (@shared/fallbacks' withBackend, which shapes a whole BotDraft): a seat on
      // the same agent keeps the model, another agent starts fresh — keep the two in step.
      if (b.id === pick.backend) return
      run.current++
      setOutcomes({})
      setPick({ backend: b.id, model: null, modelOptions: {} })
    },
  })
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
  // The window search's own match — search-items builds each bot's words, palette-items matches
  // them — so the manager's filter always agrees with the search on what a query finds.
  const shown = useMemo(() => {
    const rows = botItems(items, () => {}).map((row) => {
      if (!modelSearch) return row
      const item = items.find((candidate) => refKey(candidate.ref) === row.key)
      if (!item) return row
      const backend = item.bot.backend
      const rowBackends = computerBackends(item)
      const extra = modelPathWords(item.bot, agentNameOf(rowBackends, backend), loginNameOf(rowBackends, backend, item.bot.loginProfile))
      return { ...row, words: [...row.words, ...extra] }
    })
    const matched = new Set(matchItems(rows, query).map((row) => row.key))
    return items
      .filter((item) => matched.has(refKey(item.ref)))
      .filter((item) => !chosenProvider || item.bot.backend === chosenProvider.id)
  }, [items, query, chosenProvider, modelSearch])
  const shownKeys = allKeys(shown)
  // The filter can change while a batch awaits a catalog or an earlier save. The loops read this
  // at each write, so a bot hidden since the batch started is left untouched.
  const shownNow = useRef(new Set<string>())
  shownNow.current = new Set(shownKeys)
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

  // Pause or resume every checked bot the filter still shows. The patch is `{ id, paused }` only.
  // Trust the echo: a host that ignores the field still returns success, and that is not Paused.
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
        if (remainingTargets([item], shownNow.current).length === 0) continue
        try {
          const client = await item.store.ready()
          // The wait may outlast the run, or the filter may have hidden this bot: re-check before anything is sent.
          if (!alive.current || run.current !== myRun) continue
          if (remainingTargets([item], shownNow.current).length === 0) continue
          const res = await client.call('updateBot', pausePatch(item.ref.botId, paused))
          // The host has already answered. Count that echo even if the sheet closed while it was in flight.
          const result = countedPause(pauseEcho(paused, res).applied, alive.current && run.current === myRun)
          if (result.tally === 'applied') applied++
          else failed++
          if (result.show) {
            const text = result.tally === 'applied' ? tr(paused ? 'Paused' : 'Resumed') : tr('Not applied on this computer')
            setOutcomes((o) => ({ ...o, [key]: { ok: result.tally === 'applied', text } }))
          }
        } catch (e) {
          failed++
          if (alive.current && run.current === myRun) setOutcomes((o) => ({ ...o, [key]: { ok: false, text: errorText(e) } }))
        }
      }
    } finally {
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
    // Counted as the run goes, so the event can say what went wrong by kind, with no text and no ids: the five
    // kinds always sum to `failed`.
    const tally = runTally(chosen.length)
    // What this run changed: an applied bot leaves the selection afterwards, so a later Apply can
    // never sweep it up again by surprise.
    const appliedKeys = new Set<string>()
    const catalogs: Record<string, CatalogCheck> = {}
    // A computer is read once per login: two of its bots on different profiles can see different
    // models, so the catalog check must use the same profile the save will keep.
    const catalogKey = (item: RosterItem) => `${item.ref.computerId}\u0000${loginProfileFor(item.bot, backend) ?? ''}`
    try {
      if (needsCatalog(backend, model, options)) {
        for (const item of chosen) {
          if (!alive.current || run.current !== myRun) return
          if (remainingTargets([item], shownNow.current).length === 0) continue
          const key = catalogKey(item)
          if (key in catalogs) continue
          const loginProfile = loginProfileFor(item.bot, backend)
          catalogs[key] = await item.store
            .ready()
            .then(async (client): Promise<CatalogCheck> => {
              // The wait for a reconnect may outlast the run: re-check before anything is read.
              if (!alive.current || run.current !== myRun) return { kind: 'error' }
              const catalog = await client.call<AgentModels>('agentModels', agentModelsRequest(backend, loginProfile), 300_000)
              return { kind: 'ok', catalog }
            })
            .catch((): CatalogCheck => ({ kind: 'error' }))
        }
      }
      const plan: ApplyPlan = { items: chosen, backend, model, options }
      const rows = planRows(plan, (item) => catalogs[catalogKey(item)] ?? { kind: 'none' })
      for (const row of rows) {
        if (!alive.current || run.current !== myRun) return
        const key = refKey(row.item.ref)
        if (remainingTargets([row.item], shownNow.current).length === 0) continue
        if (!row.ok) {
          countFailure(tally, row.refusal)
          setOutcomes((o) => ({ ...o, [key]: { ok: false, text: row.reason } }))
          continue
        }
        // An older host cannot make the login expectation atomic with its save, so fail this row
        // rather than silently restoring the login the catalog check used.
        if (row.item.store.hello?.modelManagerApplyGuard !== true) {
          countFailure(tally, 'hostGuard')
          const text = tr('Update Codync on {computer} to apply models safely').replace('{computer}', () => row.item.computer.name)
          setOutcomes((o) => ({ ...o, [key]: { ok: false, text } }))
          continue
        }
        try {
          // The patch pins the login whose catalog was checked. The host requires that login to
          // still be current under the save lock, so a change from another window or device is
          // never overwritten. Other fields merge over the bot's current state as before.
          const patch = applyPatch(row.item, plan)
          await row.item.store
            .ready()
            .then((client) => {
              // The wait may outlast the run, or the filter may have hidden this bot: re-check before anything is sent.
              if (!alive.current || run.current !== myRun) return 'skipped'
              if (remainingTargets([row.item], shownNow.current).length === 0) return 'skipped'
              return client.call('updateBot', patch).then((): 'sent' | 'skipped' => 'sent')
            })
            .then((sent) => {
              if (sent !== 'sent') return
              countApplied(tally)
              appliedKeys.add(key)
              if (alive.current && run.current === myRun) setOutcomes((o) => ({ ...o, [key]: { ok: true, text: tr('Applied') } }))
            })
        } catch (e) {
          countFailure(tally, 'saveError')
          if (alive.current && run.current === myRun) setOutcomes((o) => ({ ...o, [key]: { ok: false, text: errorText(e) } }))
        }
      }
    } finally {
      busy.current = false
      setApplying(false)
      // An applied bot leaves the selection; failed or never-reached bots stay checked, so a
      // retry keeps exactly what still needs work and nothing lands twice by accident. Before the
      // analytics event: even a throwing tracker cannot leave the old selection live.
      if (alive.current && appliedKeys.size > 0)
        setChecked((set) => {
          const next = new Set(set)
          for (const key of appliedKeys) next.delete(key)
          return next
        })
      // One event per apply, whatever ended it: closed mid-run, pick changed mid-run, or completed.
      // Rows the run never reached are the remainder (requested - applied - failed).
      analytics.track({ event: 'model_manager_applied', ...tally })
    }
  }

  return (
    <VStack gap={12} className="model-manager-body">
      <ModelPicker
        trigger="row"
        backend={pick.backend}
        models={models}
        seats={seats}
        model={{ id: pick.model, options: pick.modelOptions }}
        set={(m) => { run.current++; setOutcomes({}); setPick((p) => ({ ...p, model: m.id, modelOptions: m.options })) }}
      />
      <div className="palette-field">
        <Icon name="magnifyingglass" size={15} color="var(--secondary)" />
        <input value={query} placeholder={tr(modelSearch ? 'Search bots or models' : 'Search bots')} aria-label={tr(modelSearch ? 'Search bots or models' : 'Search bots')} spellCheck={false} onChange={(e) => setQuery(e.target.value)} />
      </div>
      <HStack gap={8} className="model-manager-filters">
        <Switch on={modelSearch} onChange={setModelSearch} className="model-manager-model-search">Search models too</Switch>
        <Spacer />
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
      </HStack>
      <HStack gap={8} className="model-manager-tools" wrap>
        <Button onClick={() => markAll(true)} disabled={applying || everyChecked || shownKeys.length === 0}>{tr('Select all')}</Button>
        <Button onClick={() => markAll(false)} disabled={applying || checked.size === 0}>{tr('Unselect all')}</Button>
        <Button onClick={() => void setPausedAll(true)} disabled={!canApply}>{tr('Pause checked')}</Button>
        <Button onClick={() => void setPausedAll(false)} disabled={!canApply}>{tr('Resume checked')}</Button>
        <HStack gap={8} className="model-manager-tools-end" justify="flex-end">
          {targets.totalChecked === 0 ? <Text size="caption" tone="secondary">{tr('No bots selected')}</Text> : <Text size="caption" tone="secondary">{tr('Selected: {n}').replace('{n}', () => String(targets.totalChecked))}</Text>}
          <Button onClick={apply} disabled={!canApply}>{tr('Apply')}</Button>
        </HStack>
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
          // The bot's current agent, login and model path, under the name in the app's muted small text.
          const rowBackends = computerBackends(item)
          const agentName = agentNameOf(rowBackends, item.bot.backend)
          const loginName = loginNameOf(rowBackends, item.bot.backend, item.bot.loginProfile)
          const modelPath = rowModelPath(item.bot, agentName, loginName)
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

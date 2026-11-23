import { useContext, useRef, useState, type CSSProperties, type KeyboardEvent, type ReactNode } from 'react'
import { approvals, withApproval } from '@shared/approvals'
import { withBackend } from '@shared/fallbacks'
import { draftOf, type Bot } from '@shared/models'
import { AgentIcon } from '../../components/AgentIcon'
import { DropdownMenu, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { AnchoredMenu, type MenuItem } from '../../components/Menu'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { modelChoices, useAgentModels, type AgentModelsState, type ModelOption } from '../bots/agent-models'
import { normalized } from '../bots/drafts'
import { OpenExtensions } from '../NavRail'
import { loginSeats, type Seat } from './seats'

export interface Approval { permission: string; mode?: string | null }
export interface ModelPick { id: string | null; options: Record<string, string> }

/** A way of approving, as an icon: asking, approving everything, or the agent's own mode read from its name. */
const approvalIcon = ({ id, label }: { id: string; label: string }) =>
  id === 'ask' ? 'hand.raised'
    : id === 'approve' ? 'checkmark.shield'
      : /plan/i.test(label) ? 'list.bullet.clipboard'
        : /read/i.test(label) ? 'eye'
          : /edit/i.test(label) ? 'pencil'
            : /bypass|full|danger|yolo/i.test(label) ? 'exclamationmark.shield'
              : 'shield'

/** How it is approved, as a menu: every way the agent offers, each with its icon, and the one in use. */
export function useApprovalMenu(approval: Approval, models: AgentModelsState, set: (approval: Approval) => void) {
  const tr = useTr()
  const { choices, selected } = approvals(approval, models.catalog)
  const items = (): MenuItem[] => choices.map((c) => ({
    title: c.label, icon: approvalIcon(c), detail: c.note ?? undefined, selected: c.id === selected.id, action: () => set(withApproval(approval, c.id)),
  }))
  return { items, label: tr(selected.label), icon: approvalIcon(selected) }
}

/** The model setting that is its effort (how hard it thinks), listed in the panel's own column. */
const isEffort = (o: ModelOption) => /effort|reasoning|thinking/i.test(`${o.id} ${o.name}`)
const shortcut = window.codync.platform === 'darwin' ? '⌘' : 'Ctrl+'

/** One thing a conversation runs on, as a quiet chip that opens its choices. */
export function Option({ title, items, leading, label, disabled = false, strong = false, search }: { title: string; items: () => MenuItem[]; leading: ReactNode; label: string; disabled?: boolean; strong?: boolean; search?: string }) {
  const tr = useTr()
  return (
    <DropdownMenu items={items} title={tr(title)} className="thread-option" disabled={disabled} search={search}>
      {leading}
      <Text size="caption" tone={strong ? 'text' : 'secondary'} truncate>{label}</Text>
      {disabled ? null : <Icon name="chevron.down" size={8} color="var(--tertiary)" />}
    </DropdownMenu>
  )
}

/**
 * The model chip and its panel, the one way every place picks what runs: a rail of the agents' logins (`loginSeats`);
 * the agent's models under a search field (⌘1–9 picks one), with the effort as a slider and the model's other settings
 * under them, then `children`; and a footer of speed and how it is approved. Another model starts without settings.
 * `trigger`: a quiet chip in a message box's row, or a full-width row in a settings card.
 */
export function ModelPicker({ backend, model, models, set, fast, seats, approval, adds = 'agent', trigger = 'chip', startOpen = false, children }: {
  backend: string
  model: ModelPick
  models: AgentModelsState
  set: (model: ModelPick) => void
  /** Fast mode, where the agent has it and this chip may change it. */
  fast?: { on: boolean; set: (on: boolean) => void } | null
  /** What it may run on; without them the rail shows only the agent it runs on. */
  seats?: Seat[]
  /** How it is approved, as the footer's menu. */
  approval?: { items: () => MenuItem[]; label: string; icon: string }
  /** What the rail's `+` adds: another agent (the default), or a login profile of this one. */
  adds?: 'agent' | 'login'
  trigger?: 'chip' | 'row'
  /** Opens on mount (a fallback just added). */
  startOpen?: boolean
  /** Under the model's settings: notes and fields of the place it's in (a custom command, why it rests). */
  children?: ReactNode
}) {
  const tr = useTr()
  const store = useStore()
  const openExtensions = useContext(OpenExtensions)
  const [open, setOpen] = useState(startOpen)
  const [query, setQuery] = useState('')
  const anchor = useRef<HTMLButtonElement>(null)
  const { value, options, modelOptions } = modelChoices(models.catalog, model.id, tr)
  const chosen = (o: ModelOption) => model.options[o.id] ?? o.default ?? o.values[0]?.id
  const effort = modelOptions.find(isEffort) ?? null
  const rest = modelOptions.filter((o) => o !== effort)
  const modelName = options.find((o) => o.id === value)?.label ?? value
  const effortName = effort?.values.find((v) => v.id === chosen(effort))?.name ?? null
  const pick = (o: ModelOption, id: string) => set({ id: model.id, options: { ...model.options, [o.id]: id } })
  const choose = (id: string) => (id === value ? undefined : set({ id: id || null, options: {} }))
  const close = () => {
    setOpen(false)
    setQuery('')
  }
  const typed = query.trim()
  const found = options.filter((o) => o.label.toLowerCase().includes(typed.toLowerCase()))
  // An agent that lists no models of its own runs one named by its ID: what is typed is offered as one.
  const listed = (models.catalog?.models.length ?? 0) > 0
  const custom = typed && !options.some((o) => o.id === typed || o.label === typed) ? typed : null
  const jump = (e: KeyboardEvent) => {
    const at = (e.metaKey || e.ctrlKey) && /^[1-9]$/.test(e.key) ? found[Number(e.key) - 1] : undefined
    if (!at) return
    e.preventDefault()
    choose(at.id)
  }
  const at = effort ? Math.max(0, effort.values.findIndex((v) => v.id === chosen(effort))) : 0
  const check = <Icon name="checkmark" size={10} weight="semibold" color="var(--text)" />
  const seatName = seats?.find((s) => s.on)?.name ?? store.backendName(backend)
  const running = [modelName, effortName].filter(Boolean).join(' · ')
  const toggle = () => (open ? close() : setOpen(true))
  return (
    <>
      {trigger === 'chip' ? (
        <button ref={anchor} className="thread-option" title={tr('Model')} aria-haspopup="dialog" aria-expanded={open} onClick={toggle}>
          <AgentIcon registry={store.agentRegistry(backend)} size={12} />
          <Text size="caption" tone="secondary" truncate>{running}</Text>
          <Icon name="chevron.down" size={8} color="var(--tertiary)" />
        </button>
      ) : (
        <button ref={anchor} className="model-picker-field" title={`${seatName} · ${running}`} aria-label={tr('Model')} aria-haspopup="dialog" aria-expanded={open} onClick={toggle}>
          <AgentIcon registry={store.agentRegistry(backend)} size={14} />
          <Text tone="text" truncate>{seatName}</Text>
          <Text tone="secondary" truncate grow>{running}</Text>
          {fast?.on ? <Icon name="bolt" size={11} title={tr('Fast mode')} /> : null}
          <Icon name="chevron.down" size={10} color="var(--tertiary)" />
        </button>
      )}
      <AnchoredMenu open={open} onClose={close} anchor={anchor} width={380} className="model-picker">
        <div className="model-picker-columns" onKeyDown={jump}>
          <div className="model-picker-rail" role="toolbar" aria-label={tr('Agent')}>
            {(seats ?? [{ key: backend, backend, name: store.backendName(backend), on: true, pick: () => {} }]).map((s) => (
              <button key={s.key} className="model-picker-agent" title={s.name} aria-label={s.name} aria-pressed={s.on} disabled={!seats} onClick={() => (s.on ? undefined : s.pick())}>
                <AgentIcon registry={store.agentRegistry(s.backend)} size={18} />
                {s.badge ? <span className="model-picker-badge">{s.badge}</span> : null}
              </button>
            ))}
            {/* Another agent or login is added (installed, signed in) in Extensions, then shows here. */}
            {openExtensions ? (
              <button className="model-picker-agent model-picker-add" title={tr(adds === 'login' ? 'Add login profile' : 'Add agent')} aria-label={tr(adds === 'login' ? 'Add login profile' : 'Add agent')}
                onClick={() => { close(); openExtensions(store.computer.id, adds === 'login' ? { section: 'logins', agent: backend } : { section: 'agents' }) }}>
                <Icon name="plus" size={13} color="var(--secondary)" />
              </button>
            ) : null}
          </div>
          <div className="model-picker-models">
            <label className="model-picker-search">
              <Icon name="magnifyingglass" size={12} color="var(--tertiary)" />
              <input autoFocus value={query} placeholder={tr('Search models')} aria-label={tr('Search models')} onChange={(e) => setQuery(e.target.value)} />
              {/* The agent's models are still coming from the computer; or asked for again. */}
              {models.loading ? <Spinner size={12} /> : <IconButton title={models.failure ? 'Retry model list' : 'Refresh model list'} icon="arrow.clockwise" size={20} onClick={models.reload} />}
            </label>
            <div className="model-picker-list" role="listbox" aria-label={tr('Model')}>
              {found.map((o, i) => (
                <button key={o.id} role="option" aria-selected={o.id === value} className="model-picker-row" onClick={() => choose(o.id)}>
                  <AgentIcon registry={store.agentRegistry(backend)} size={14} />
                  <Text size="callout" tone="text" truncate>{o.label}</Text>
                  {o.id === value ? check : null}
                  <span className="model-picker-space" />
                  {i < 9 ? <kbd className="model-picker-kbd">{shortcut}{i + 1}</kbd> : null}
                </button>
              ))}
              {custom ? (
                <button role="option" aria-selected={false} className="model-picker-row" onClick={() => { choose(custom); setQuery('') }}>
                  <Icon name="plus" size={11} color="var(--secondary)" />
                  <Text size="callout" tone="text" truncate>{custom}</Text>
                  <span className="model-picker-space" />
                  <Text size="caption" tone="tertiary">{tr('Model ID')}</Text>
                </button>
              ) : null}
              {models.loading || (found.length && listed) || custom ? null : (
                <Text as="div" size="caption" tone="tertiary" className="model-picker-empty">
                  {models.failure ?? (listed ? tr('No results') : tr("This agent doesn't advertise a model list. Leave Default or enter a model ID."))}
                </Text>
              )}
            </div>
            {/* Under the models, the effort: its name and level over a track of steps whose knob slides, the steps up
                to it filled with a fine dot grid; then the model's other settings, each its name and a menu of values. */}
            {effort && effort.values.length > 1 ? (
              <div className="model-picker-effort">
                <div className="model-picker-effort-head">
                  <Text size="caption" tone="secondary">{effort.name}</Text>
                  <Text size="caption" tone="text" weight="medium">{effortName}</Text>
                </div>
                <div className="model-effort" role="radiogroup" aria-label={effort.name} style={{ '--at': at / (effort.values.length - 1) } as CSSProperties}>
                  <span className="model-effort-fill" />
                  <span className="model-effort-knob" />
                  {effort.values.map((v) => (
                    <button key={v.id} role="radio" aria-checked={v.id === chosen(effort)} title={v.name} className="model-effort-step" onClick={() => pick(effort, v.id)}>
                      <span className="model-effort-dot" />
                    </button>
                  ))}
                </div>
              </div>
            ) : null}
            {rest.map((o) => (
              <DropdownMenu
                key={o.id}
                title={o.name}
                className="model-picker-setting"
                items={() => o.values.map((v) => ({ title: v.name, verbatim: true, selected: v.id === chosen(o), action: () => pick(o, v.id) }))}
              >
                <Text size="caption" tone="secondary" grow truncate>{o.name}</Text>
                <Text size="caption" tone="text" truncate>{o.values.find((v) => v.id === chosen(o))?.name ?? ''}</Text>
                <Icon name="chevron.up.chevron.down" size={9} color="var(--tertiary)" />
              </DropdownMenu>
            ))}
            {children ? <div className="model-picker-extra">{children}</div> : null}
          </div>
        </div>
        {fast || approval ? (
          <div className="model-picker-footer">
            {fast ? (
              <div className="model-picker-speed" role="radiogroup" aria-label={tr('Fast mode')}>
                <button role="radio" aria-checked={!fast.on} onClick={() => fast.set(false)}>{tr('Standard')}</button>
                <button role="radio" aria-checked={fast.on} onClick={() => fast.set(true)}>
                  <Icon name="bolt" size={10} />
                  {tr('Fast')}
                </button>
              </div>
            ) : null}
            {approval ? (
              <DropdownMenu items={approval.items} title={tr('Permissions')} className="model-picker-permissions">
                <Icon name={approval.icon} size={11} />
                <Text size="caption" tone="secondary" truncate>{approval.label}</Text>
                <Icon name="chevron.up" size={8} color="var(--tertiary)" />
              </DropdownMenu>
            ) : null}
          </div>
        ) : null}
      </AnchoredMenu>
    </>
  )
}

/** What a bot's chat runs on, in its message box: its model, with every agent's logins in the panel's rail (adding one
 * stays in Settings), Fast mode where the agent has it, and how it is approved; each saved to the bot as its settings would. */
export function BotRunOptions({ bot }: { bot: Bot }) {
  const tr = useTr()
  const store = useStore()
  const models = useAgentModels(bot.backend, bot.loginProfile)
  const save = (change: Partial<ReturnType<typeof draftOf>>) => void store.save(normalized({ ...draftOf(bot), ...change })).catch((e: unknown) => store.setError(String(e)))
  const ways = useApprovalMenu({ permission: bot.permission, mode: bot.mode }, models, (a) => save({ permission: a.permission, mode: a.mode ?? null }))
  const backends = store.hello?.backends ?? []
  const agent = backends.find((b) => b.id === bot.backend)
  // Another agent is the bot's agent changed, as in its settings.
  const seats: Seat[] = loginSeats({ backends, on: bot, tr, pick: (b, loginProfile) => save({ ...withBackend(draftOf(bot), b.id), loginProfile }) })
  return (
    <ModelPicker
      backend={bot.backend}
      model={{ id: bot.model ?? null, options: bot.modelOptions }}
      models={models}
      set={(m) => save({ model: m.id, modelOptions: m.options })}
      fast={models.catalog?.fastMode || bot.fastMode ? { on: bot.fastMode, set: (on) => save({ fastMode: on }) } : null}
      approval={ways}
      seats={seats}
      adds={agent?.loginProfiles ? 'login' : 'agent'}
    />
  )
}

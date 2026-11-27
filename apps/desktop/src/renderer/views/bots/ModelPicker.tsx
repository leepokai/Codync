import { useContext, useRef, useState, type KeyboardEvent, type ReactNode } from 'react'
import { AgentIcon } from '../../components/AgentIcon'
import { DropdownMenu, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { AnchoredMenu, type MenuItem } from '../../components/Menu'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { OpenExtensions } from '../NavRail'
import { EffortTrack } from './EffortTrack'
import type { ModelOption } from '@shared/agent-models'
import { modelChoices, type AgentModelsState } from './agent-models'
import type { Seat } from './seats'
import './model-picker.css'

export interface ModelPick { id: string | null; options: Record<string, string> }

/** The model setting that is its effort (how hard it thinks), listed in the panel's own column. */
const isEffort = (o: ModelOption) => /effort|reasoning|thinking/i.test(`${o.id} ${o.name}`)
const shortcut = window.codync.platform === 'darwin' ? '⌘' : 'Ctrl+'

/**
 * The model chip and its panel, the one way every place picks what runs: a rail of the agents' logins (`loginSeats`);
 * the agent's models under a search field (⌘1–9 picks one), with the effort as a slider and the model's other settings
 * under them, then `children`; and a footer of speed and how it is approved. Another model starts without settings.
 * `trigger`: a quiet chip in a message box's row, or a full-width row in a settings card.
 */
export function ModelPicker({ backend, model, models, set, fast, seats, approval, trigger = 'chip', startOpen = false, children }: {
  backend: string
  model: ModelPick
  models: AgentModelsState
  set: (model: ModelPick) => void
  /** Fast mode, where the agent has it and this chip may change it. */
  fast?: { on: boolean; set: (on: boolean) => void } | null
  /** What it may run on, the rail's. */
  seats: Seat[]
  /** How it is approved, as the footer's menu. */
  approval?: { items: () => MenuItem[]; label: string; icon: string }
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
  const check = <Icon name="checkmark" size={10} weight="semibold" color="var(--text)" />
  const seatName = seats.find((s) => s.on)?.name ?? store.backendName(backend)
  // The rail's `+` adds a login profile of an agent that has them, else another agent.
  const adds = store.hello?.backends.find((b) => b.id === backend)?.loginProfiles ? 'login' : 'agent'
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
            {seats.map((s) => (
              <button key={s.key} className="model-picker-agent" title={s.name} aria-label={s.name} aria-pressed={s.on} onClick={() => (s.on ? undefined : s.pick())}>
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
                <EffortTrack option={effort} chosen={chosen(effort)} onPick={(id) => pick(effort, id)} />
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

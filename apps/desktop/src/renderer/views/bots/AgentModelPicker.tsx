import { useState, type ReactNode } from 'react'
import { IconButton, SegmentedChoice, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import type { AgentModelsState } from './agent-models'
import { useTr } from '../../lib/use-language'
import './agent-picker.css'
import { isReasoningOption, ReasoningSlider } from './ReasoningSlider'

type Stage = 'closed' | 'settings' | 'agents' | 'models'
type Choice = { id: string; label: string; leading?: ReactNode }

/** One compact entry for the harness, model, model options and the bot's Fast mode. */
export function AgentModelPicker({ backend, models, selection, onChange, options: chosen, onOptionsChange,
  sessionNote = true, agents = [], onAgentChange, fastControl, fast = false, children }: {
  backend: string
  models: AgentModelsState
  selection: string | null | undefined
  onChange: (model: string | null) => void
  options: Record<string, string>
  onOptionsChange: (options: Record<string, string>) => void
  sessionNote?: boolean
  agents?: Choice[]
  onAgentChange?: (id: string) => void
  fastControl?: ReactNode
  fast?: boolean
  children?: ReactNode
}) {
  const tr = useTr()
  const [stage, setStage] = useState<Stage>('closed')
  const [query, setQuery] = useState('')
  const { catalog, loading, failure } = models
  const agentDefault = catalog?.models.find((m) => m.name.toLowerCase().startsWith('default'))
  const available = (catalog?.models ?? []).filter((m) => m.id !== agentDefault?.id)
  const current = catalog?.models.find((m) => m.id === catalog.currentModelId)
  const value = selection === agentDefault?.id ? '' : (selection ?? '')
  const selected = catalog?.models.find((m) => m.id === (value || agentDefault?.id || catalog.currentModelId))
  const modelOptions = value || catalog?.defaultModelOptions ? selected?.options ?? [] : []
  const options: Choice[] = [{ id: '', label: agentDefault?.name ?? (current ? `Default · ${current.name}` : 'Default') }, ...available.map((m) => ({ id: m.id, label: m.name }))]
  if (value && !available.some((m) => m.id === value)) options.push({ id: value, label: value })
  const agent = agents.find((a) => a.id === backend)
  const modelName = value ? options.find((o) => o.id === value)?.label ?? value : tr('Default')
  const summary = modelOptions.map((o) => o.values.find((v) => v.id === (chosen[o.id] ?? o.default))?.name).filter(Boolean).join(' · ')
  const browse = stage === 'agents' || stage === 'models'
  const choices = (stage === 'agents' ? agents : options).filter((o) => o.label.toLocaleLowerCase().includes(query.toLocaleLowerCase()))
  const open = (next: Stage) => { setQuery(''); setStage(next) }
  const pick = (id: string) => {
    if (stage === 'agents') {
      if (id !== backend) onAgentChange?.(id)
      open('models')
    } else {
      if (id !== value) onChange(id || null)
      open('settings')
    }
  }
  const setOption = (id: string, value?: string) => {
    const next = { ...chosen }
    if (value === undefined) delete next[id]
    else next[id] = value
    onOptionsChange(next)
  }
  return (
    <VStack gap={8} className="agent-picker">
      <button className="agent-picker-summary" title={[agent?.label ?? backend, modelName, summary, fast ? tr("Fast") : ""].filter(Boolean).join(" · ")} aria-expanded={stage !== 'closed'} onClick={() => open(stage === 'closed' ? 'models' : 'closed')}>
        <span className="agent-picker-identity">{agent?.leading}<Text key={backend} className="agent-picker-text-change">{agent?.label ?? backend}</Text></span>
        <span className="agent-picker-model" title={modelName}><span key={modelName} className="agent-picker-text-change">{modelName}</span></span>
        {summary && <span className="agent-picker-effort" title={summary}><span key={summary} className="agent-picker-text-change">{summary}</span></span>}
        {fast && <Icon name="bolt" size={12} title={tr('Fast mode')} />}
        <Icon name={stage === 'closed' ? 'chevron.down' : 'chevron.up'} size={10} />
      </button>
      {stage !== 'closed' && <VStack gap={10} className="agent-picker-panel fade-in">
        <div className="agent-picker-selectors">
          {onAgentChange && <button className="agent-picker-choice" title={agent?.label ?? backend} aria-expanded={stage === 'agents'} onClick={() => open(stage === 'agents' ? 'settings' : 'agents')}>{agent?.leading}<Text truncate>{agent?.label ?? backend}</Text><Icon name={stage === 'agents' ? 'chevron.up' : 'chevron.down'} size={10} /></button>}
          <button className="agent-picker-choice" title={options.find((o) => o.id === value)?.label} aria-expanded={stage === 'models'} onClick={() => open(stage === 'models' ? 'settings' : 'models')}><Text truncate>{modelName}</Text><Icon name={stage === 'models' ? 'chevron.up' : 'chevron.down'} size={10} /></button>
        </div>
        {children}
        {browse && <VStack gap={4}>
          <HStack gap={6} className="agent-picker-search">
            {stage === 'models' && onAgentChange ? <IconButton title="Back" icon="chevron.left" onClick={() => open('agents')} /> : <Icon name="magnifyingglass" size={14} />}
            <input aria-label={tr(stage === 'agents' ? 'Search agents' : 'Search models')} placeholder={tr(stage === 'agents' ? 'Search agents' : 'Search models')} value={query} onChange={(e) => setQuery(e.target.value)} />
            {loading ? <Spinner size={12} /> : <IconButton title="Refresh model list" icon="arrow.clockwise" onClick={models.reload} />}
          </HStack>
          <div className="agent-picker-list">
            {choices.map((o) => <button key={o.id} className="agent-picker-item" aria-pressed={o.id === (stage === 'agents' ? backend : value)} onClick={() => pick(o.id)}>
              {o.leading}<Text className="agent-picker-result-name">{tr(o.label)}</Text><Spacer />{o.id === (stage === 'agents' ? backend : value) && <Icon name="checkmark" size={12} />}
            </button>)}
            {!choices.length && <Text size="caption" tone="secondary">{tr('No results')}</Text>}
          </div>
          {stage === 'models' && (backend === 'custom' || (!loading && !catalog?.models.length)) && <input className="field-box" aria-label={tr('Model')} placeholder={tr('Default')} value={selection ?? ''} onChange={(e) => onChange(e.target.value || null)} />}
        </VStack>}
        {selected?.optionsError && <Text size="caption" tone="attention">{selected.optionsError}</Text>}
        {failure && <Text size="caption" tone="attention">{failure}</Text>}
        {modelOptions.map((o) => {
          if (isReasoningOption(o)) return <ReasoningSlider key={o.id} option={o} value={chosen[o.id]} onChange={(v) => setOption(o.id, v)} onReset={() => setOption(o.id)} />
          const defaultName = o.values.find((v) => v.id === o.default)?.name
          return <VStack key={o.id} gap={8} className="agent-picker-setting">
            <HStack gap={8}><Text size="caption" tone="secondary">{o.name}</Text><Spacer />
              <button className="agent-picker-default" aria-pressed={chosen[o.id] === undefined} onClick={() => setOption(o.id)}>{tr('Default')}{defaultName ? ` · ${defaultName}` : ''}{chosen[o.id] === undefined ? ' ✓' : ''}</button>
            </HStack>
            <div className="agent-picker-options"><SegmentedChoice selection={chosen[o.id] ?? o.default ?? ''} options={o.values.map((v) => ({ id: v.id, label: v.name }))} onChange={(v) => setOption(o.id, v)} /></div>
          </VStack>
        })}
        {fastControl}
        {sessionNote && <Text size="caption" tone="secondary" className="agent-picker-note">{tr('Changing the model starts a new agent session. Chat history is kept.')}</Text>}
      </VStack>}
    </VStack>
  )
}

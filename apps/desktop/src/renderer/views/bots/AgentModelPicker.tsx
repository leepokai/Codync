import { useEffect, useRef } from 'react'
import { ChoicePicker, IconButton, Pill, Spinner } from '../../components/Controls'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { effortRow, modelSettings, stepSettings, type ModelSetting, type SessionReport } from '@shared/model-settings'
import type { AgentModelsState } from './agent-models'

/** The bot's model and its options, from the list `useAgentModels` loaded for its agent: the Effort level row first,
 *  then the agent's other settings under their own names. */
export function AgentModelPicker({ backend, models, selection, onChange, options: chosen, onOptionsChange, sessionNote = true, status, working = false }: {
  backend: string
  models: AgentModelsState
  selection: string | null | undefined
  onChange: (model: string | null) => void
  options: Record<string, string>
  onOptionsChange: (options: Record<string, string>) => void
  /** Says that a change starts a new session: true for a bot's own model, not for one it hasn't used yet. */
  sessionNote?: boolean
  /** What the running agent says, for the Effort level caption: its report when this step runs (else null), and the
   *  options the host has saved. Absent: no caption (a fallback being added, an unsaved model or agent). */
  status?: { report: SessionReport | null; saved: Record<string, string> }
  /** The bot is replying: a change waits for the reply. */
  working?: boolean
}) {
  const { catalog, loading, failure } = models
  const custom = backend === 'custom'

  // A different agent starts on its own default model.
  const lastBackend = useRef(backend)
  useEffect(() => {
    if (lastBackend.current === backend) return
    lastBackend.current = backend
    onChange(null)
    // Its settings are another agent's too.
    onOptionsChange({})
  }, [backend, onChange, onOptionsChange])

  // The agent's own "Default (recommended)" entry is the same choice as leaving the model unset,
  // so it folds into the single Default row instead of showing twice.
  const agentDefault = catalog?.models.find((m) => m.name.toLowerCase().startsWith('default'))
  const available = (catalog?.models ?? []).filter((m) => m.id !== agentDefault?.id)
  const current = available.find((m) => m.id === catalog?.currentModelId)?.name
  const options = [{ id: '', label: agentDefault?.name ?? (current ? `Default · ${current}` : 'Default') }, ...available.map((m) => ({ id: m.id, label: m.name }))]
  if (selection && selection !== agentDefault?.id && !available.some((m) => m.id === selection)) options.push({ id: selection, label: selection })
  const value = selection === agentDefault?.id ? '' : (selection ?? '')
  const set = (v: string) => onChange(v ? v : null)
  const empty = catalog?.models.length === 0
  const catalogSettings = modelSettings(catalog, value)
  const report = status?.report ?? null
  const effortId = effortRow({ catalogSettings, saved: undefined, report, loading, failed: !!failure, chosenModel: !!value }).setting?.id
  const effort = effortRow({
    catalogSettings,
    saved: effortId ? (status?.saved[effortId] ?? chosen[effortId]) : undefined,
    report,
    loading,
    failed: !!failure,
    chosenModel: !!value,
  })
  // A value not saved yet has nothing to report.
  const unsaved = !status || (effortId !== undefined && (chosen[effortId] ?? '') !== (status.saved[effortId] ?? ''))
  // The rest of what the running session lists, and any saved setting nobody lists (so it can be cleared).
  const others = stepSettings(catalogSettings, report, chosen).filter((o) => o.id !== effort.setting?.id)
  const setOption = (id: string, v: string) => {
    const { [id]: _, ...rest } = chosen
    onOptionsChange(v ? { ...rest, [id]: v } : rest)
  }

  return (
    <VStack gap={6}>
      <HStack gap={8}>
        <Text tone="text" className="model-picker-label">Model</Text>
        <Spacer />
        <HStack gap={4} shrink>
          {custom ? null : (
            // Loading and refresh share one fixed slot, so the row never grows for loading.
            <div className="model-picker-slot">
              <div className={`model-picker-refresh ${loading ? 'hidden' : ''}`}>
                <IconButton title={failure ? 'Retry model list' : 'Refresh model list'} icon="arrow.clockwise" disabled={loading} onClick={models.reload} />
              </div>
              {loading ? (
                <HStack align="stretch" className="fade-in" aria-label="Loading available models">
                  <Spinner size={12} />
                </HStack>
              ) : null}
            </div>
          )}
          {custom || (!loading && (!catalog || empty)) ? (
            <Pill outlined className="model-picker-pill">
              <input className="model-picker-input" value={value} placeholder="Default" spellCheck={false} onChange={(e) => set(e.target.value)} />
            </Pill>
          ) : (
            <HStack align="stretch" shrink>
              <ChoicePicker selection={value} options={options} onChange={set} />
            </HStack>
          )}
        </HStack>
      </HStack>
      {effort.kind === 'later' ? (
        <Text size="caption" tone="secondary" className="fade-in">Effort level appears once the agent runs this model.</Text>
      ) : effort.kind === 'row' ? (
        <VStack gap={2} className="fade-in">
          {effort.setting ? <SettingRow label="Effort level" setting={effort.setting} value={chosen[effort.setting.id] ?? ''} onChange={(v) => setOption(effort.setting!.id, v)} /> : null}
          {effort.caption && !unsaved ? <Text size="caption" tone="secondary">{effort.caption}</Text> : null}
        </VStack>
      ) : null}
      {others.map((o) => (
        <SettingRow key={o.id} label={o.name} setting={o} value={chosen[o.id] ?? ''} onChange={(v) => setOption(o.id, v)} />
      ))}
      {!custom && !loading && failure ? (
        <Text size="caption" tone="attention">{failure}</Text>
      ) : !custom && !loading && empty ? (
        <Text size="caption" tone="secondary">This agent doesn't advertise a model list. Leave Default or enter a model ID.</Text>
      ) : null}
      {sessionNote ? (
        <Text size="caption" tone="secondary">
          {working
            ? 'Changes to the model or effort level apply after this reply, in a new agent session. Chat history is kept; the agent’s session context starts fresh.'
            : 'Changes to the model or effort level apply to the next message, in a new agent session. Chat history is kept; the agent’s session context starts fresh.'}
        </Text>
      ) : null}
    </VStack>
  )
}

/** One setting the agent advertises: Default (its own, as last checked), then only the values it lists. */
export function SettingRow({ label, setting, value, onChange }: { label: string; setting: ModelSetting; value: string; onChange: (value: string) => void }) {
  const fallback = setting.values.find((v) => v.id === setting.default)?.name
  const choices = [{ id: '', label: fallback ? `Default · ${fallback}` : 'Default' }, ...setting.values.map((v) => ({ id: v.id, label: v.name }))]
  // A saved value the agent no longer lists stays visible until it's changed.
  if (value && !setting.values.some((v) => v.id === value)) choices.push({ id: value, label: value })
  return (
    <HStack gap={8} className="fade-in">
      <Text tone="text" className="model-picker-label">{label}</Text>
      <Spacer />
      <HStack align="stretch" shrink>
        <ChoicePicker selection={value} options={choices} onChange={onChange} />
      </HStack>
    </HStack>
  )
}

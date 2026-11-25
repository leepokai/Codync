import { useCallback, useEffect, useRef, useState } from 'react'
import { fastModeRefusal, fastModeShown } from '@shared/fast-mode'
import { restLine, restOf, withAdvisor, withBackend } from '@shared/fallbacks'
import { draftOf, type Bot, type BotDraft } from '@shared/models'
import { MAX_THREADS } from '@shared/work'
import { CharacterAvatar } from '../../components/Avatar'
import { CardSection, DropdownMenu, IconButton, Pill, Spinner, Stepper, ValueRow } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader, Sheet, SheetPage, useDismiss } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { controlReady } from '../../lib/screen-state'
import { useStore } from '../../store/context'
import { useAgentModels } from './agent-models'
import { ModelPicker } from './ModelPicker'
import { loginSeats } from './seats'
import { ApprovalRow } from './ApprovalRow'
import { CompactionRow } from './CompactionRow'
import { AgentStepBanner, FallbacksRow } from './Fallbacks'
import { ThreadsAgentRow } from './ThreadsAgentRow'
import { errorText, isValid, lastPathComponent, normalized, useFollowSaved, usePlugins, type Plugins } from './drafts'
import { FormSheet } from '../marketplace/pieces'
import { SkillSheet, type SkillRef } from '../marketplace/SkillSheet'
import { AnchoredPanel, AutoTextArea, AvatarPicker, Field, SwitchRow } from './parts'
import { NetworkSwitches } from './NetworkSwitches'
import { FolderPicker } from './FolderPicker'
import { GitStateLine } from './GitStateLine'
import { HomeFolderRow, ProjectsRow, WorktreeModeRow } from './ProjectsRows'
import { withProject } from './projects'
import './bots.css'
import { useTr } from '../../lib/use-language'
import { CustomIconEditor } from './CustomIconEditor'


/** Edit a bot in a modal: the settings form plus Close / Save. New bots start in `NewBotView`. */
export function BotEditorView({ draft: initial }: { draft: BotDraft }) {
  const tr = useTr()
  const store = useStore()
  const dismiss = useDismiss()
  const plugins = usePlugins(store)
  const [draft, setDraft] = useState(initial)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const online = store.connection.kind === 'online'
  useFollowSaved(initial.id ? store.bots.get(initial.id) : undefined, setDraft)

  const save = () => {
    if (!isValid(draft) || saving || !online) return
    setSaving(true)
    setError(null)
    store
      .save(normalized(draft))
      .then(dismiss)
      .catch((e: unknown) => setError(errorText(e)))
      .finally(() => setSaving(false))
  }

  return (
    <SheetPage>
      <ModalHeader
        title={tr(`Settings`)}
        trailing={saving ? <Spinner /> : <IconButton title="Save" icon="checkmark" disabled={!isValid(draft) || saving || !online} onClick={save} />}
      />
      <div className="bot-editor-body">
        <BotSettingsForm draft={draft} onChange={setDraft} error={error} plugins={plugins} />
      </div>
    </SheetPage>
  )
}

/** The Settings tab of the desktop inspector next to a conversation: the same form, saved as you edit. */
export function BotSettingsPanel({ botId }: { botId: string }) {
  const store = useStore()
  const plugins = usePlugins(store)
  const [draft, setDraft] = useState<BotDraft | null>(null)
  const [error, setError] = useState<string | null>(null)
  const change = useCallback((update: (d: BotDraft) => BotDraft) => setDraft((d) => (d ? update(d) : d)), [])

  useEffect(() => {
    const bot = store.bots.get(botId)
    setDraft(bot ? draftOf(bot) : null)
    // Only a different bot reloads the form; the host's echo of a save must not undo typing.
  }, [botId])
  useFollowSaved(store.bots.get(botId), change)

  // Autosave a moment after the last change.
  useEffect(() => {
    const bot = store.bots.get(botId)
    if (!draft || !bot || JSON.stringify(draft) === JSON.stringify(draftOf(bot)) || !isValid(draft)) return
    const timer = setTimeout(() => {
      store
        .save(normalized(draft))
        .then(() => setError(null))
        .catch((e: unknown) => setError(errorText(e)))
    }, 600)
    return () => clearTimeout(timer)
  }, [draft])

  return draft ? <BotSettingsForm draft={draft} onChange={change} error={error} plugins={plugins} inspector /> : null
}

/** The bot that created this one with its team tools, and when. */
function CreatedBy({ bot, creator }: { bot: Bot; creator: string }) {
  const tr = useTr()
  const store = useStore()
  const date = new Date(bot.createdAt).toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' })
  return (
    <ValueRow label="Created by">
      <Text truncate>{`${store.bots.get(creator)?.name ?? tr(`A bot`)} · ${date}`}</Text>
    </ValueRow>
  )
}

/** Bot settings, in sections in the Settings style (`CardSection`): how the bot is set up, what it is told and what
 * runs it first. In the `inspector` the Computer tab holds the Use the computer switch. */
function BotSettingsForm({ draft, onChange, error, plugins, inspector = false }: {
  draft: BotDraft
  onChange: (update: (d: BotDraft) => BotDraft) => void
  error: string | null
  plugins: Plugins | null
  inspector?: boolean
}) {
  const tr = useTr()
  const store = useStore()
  const [pickingAvatar, setPickingAvatar] = useState(false)
  const [pickingFolder, setPickingFolder] = useState(false)
  const [viewingSkill, setViewingSkill] = useState<SkillRef | null>(null)
  const avatarRef = useRef<HTMLButtonElement>(null)
  const set = <K extends keyof BotDraft>(key: K, value: BotDraft[K]) => onChange((d) => ({ ...d, [key]: value }))
  const setModel = useCallback((model: string | null, modelOptions: Record<string, string>) => onChange((d) => ({ ...d, model, modelOptions })), [onChange])
  const models = useAgentModels(draft.backend, draft.loginProfile)
  const saved = draft.id ? (store.bots.get(draft.id) ?? null) : null

  const backends = store.hello?.backends ?? []
  const backend = backends.find((b) => b.id === draft.backend)
  const connectors = plugins?.connectors ?? []
  const skills = plugins?.skills ?? []

  const instructions = (
    <Field label="Standing instructions" detail="Rules that always apply. Put task-specific requests in the chat instead.">
      <AutoTextArea className="field-box" value={draft.description} placeholder="e.g. Reviews PRs. Never pushes without asking." minRows={3} maxRows={8} onChange={(v) => set('description', v)} />
    </Field>
  )
  const ownRest = saved ? restOf(saved, draft, Date.now()) : null
  const fastShown = (draft.backend !== 'custom' && models.catalog?.fastMode) || draft.fastMode || (saved?.backend === draft.backend && fastModeShown(saved))
  const refusal = saved ? fastModeRefusal(saved) : null
  // Every agent's logins, then a custom command.
  const seats = [
    ...loginSeats({ backends, on: draft, tr, pick: (b, loginProfile) => onChange((d) => withBackend(d, { backend: b.id, loginProfile })) }),
    { key: 'custom', backend: 'custom', name: tr('Custom command'), on: draft.backend === 'custom', pick: () => onChange((d) => withBackend(d, { backend: 'custom' })) },
  ]
  const agent = (
    <ModelPicker key="agent" trigger="row" backend={draft.backend} models={models} seats={seats}
      model={{ id: draft.model ?? null, options: draft.modelOptions ?? {} }} set={(m) => setModel(m.id, m.options)}
      fast={fastShown ? { on: draft.fastMode ?? false, set: (on) => set('fastMode', on) } : null}>
      {ownRest && <Text size="caption" tone="secondary">{tr(restLine(ownRest, Date.now()))}</Text>}
      {backend && !backend.available && <Text size="caption" tone="attention">{backend.installHint}</Text>}
      {draft.backend === 'custom' && <input className="field-box bot-command" value={draft.command ?? ''} placeholder={tr('ACP command, e.g. my-agent --acp')} spellCheck={false} onChange={(e) => set('command', e.target.value || null)} />}
      {fastShown && refusal ? <Text size="caption" tone="attention">{refusal}</Text> : null}
      <Text size="caption" tone="secondary">{tr('Changing the model starts a new agent session. Chat history is kept.')}</Text>
    </ModelPicker>
  )
  // While the bot runs on a fallback, the Agent card starts with it.
  const step = saved?.agentStep ? <AgentStepBanner key="step" bot={saved} /> : null
  // Only from a host that has them (its bots say).
  const fallbacks = saved && saved.selfFallbacks !== null ? [
    <FallbacksRow key="fallbacks" draft={draft} bot={saved} onChange={onChange} />,
    <SwitchRow
      key="self"
      title="Let the bot change its fallbacks"
      detail="It can set this list itself, for example when you ask it to."
      on={draft.selfFallbacks ?? true}
      onChange={(on) => set('selfFallbacks', on)}
    />,
  ] : null
  // Advisor mode, from a host that has it: its threads' agent and fallbacks show once it's on, in its row (one setting,
  // no lines inside it).
  const advisor = saved && saved.threadFallbacks !== null ? (
    <VStack key="advisor" gap={16}>
      <SwitchRow
        title="Advisor mode"
        detail="The chat plans and hands work on; threads run on an agent of their own, with its own fallbacks."
        on={!!draft.threadAgent}
        onChange={(on) => onChange((d) => withAdvisor(d, on, backends))}
      />
      {draft.threadAgent ? <>
        <ThreadsAgentRow agent={draft.threadAgent} bot={saved} onChange={onChange} />
        <FallbacksRow threads draft={draft} bot={saved} onChange={onChange} />
      </> : null}
    </VStack>
  ) : null
  // Only where Codync summarizes the chat (its bot says): CardSection gives every child a row.
  const compaction = saved && saved.compaction !== null ? <CompactionRow key="compaction" draft={draft} bot={saved} onChange={onChange} /> : null

  return (
    <div className="bot-form">
      <div className="bot-form-content">
        <CardSection title="Instructions" fold="bot.instructions">{instructions}</CardSection>
        <CardSection title="Agent" fold="bot.agent">
          {step}
          {agent}
          {fallbacks}
          {advisor}
          {compaction}
        </CardSection>
        <CardSection title="Profile" fold="bot.profile">
          <ValueRow label="Avatar">
            <button ref={avatarRef} className="avatar-button small" title={tr('Edit Bot avatar')} aria-label={tr('Edit Bot avatar')} onClick={() => setPickingAvatar((p) => !p)}>
              <CharacterAvatar shape={draft.avatarShape} color={draft.avatarColor} size={24} customIcon={draft.id ? store.icons[`bot:${draft.id}`] : undefined} />
            </button>
            <AnchoredPanel open={pickingAvatar} onClose={() => setPickingAvatar(false)} anchor={avatarRef}>
              <div className="avatar-picker-panel">
                <AvatarPicker shape={draft.avatarShape} color={draft.avatarColor} onShape={(s) => set('avatarShape', s)} onColor={(c) => set('avatarColor', c)} />
                {draft.id && <CustomIconEditor target={`bot:${draft.id}`} />}
              </div>
            </AnchoredPanel>
          </ValueRow>
          <Field label="Name">
            <input className="field-box" value={draft.name} placeholder={tr('Name')} onChange={(e) => set('name', e.target.value)} />
          </Field>
          {/* Only a row when a bot made it: CardSection gives every child a row. */}
          {saved?.createdBy ? <CreatedBy bot={saved} creator={saved.createdBy} /> : null}
        </CardSection>

        <CardSection title="Workspace" fold="bot.workspace">
          <ValueRow label="Computer">
            <Text truncate>{store.hostName}</Text>
          </ValueRow>
          {/* A host whose bots have projects keeps each in its own folder, which a new bot gets once it's created. */}
          {store.hasProjects ? [
            saved ? <HomeFolderRow key="home" bot={saved} /> : null,
            <ProjectsRow key="projects" draft={draft} bot={saved} sharesFolders={store.fillsWorktrees} onChange={onChange} onAdd={() => setPickingFolder(true)} />,
            // Under the list, only where it matters: a project in a git repository.
            saved?.projectGitRepos.some(Boolean) ? <WorktreeModeRow key="worktrees" draft={draft} bot={saved} onChange={onChange} /> : null,
          ] : (
            <VStack gap={6}>
              <ValueRow
                label="Workspace"
                detail={
                  draft.cwd
                    ? 'This project is the default starting folder. The bot can work elsewhere when you ask.'
                    : 'This bot has its own space for files. It can work in other folders when you ask.'
                }
              >
                <DropdownMenu
                  title={draft.cwd || tr('A persistent workspace allocated for this bot')}
                  items={() => [
                    { title: 'Own folder', selected: !draft.cwd, action: () => set('cwd', '') },
                    { title: 'Choose project folder…', selected: !!draft.cwd, action: () => setPickingFolder(true) },
                  ]}
                >
                  <Pill outlined className="workspace-pill">
                    <Text truncate className="workspace-pill-label">{draft.cwd ? lastPathComponent(draft.cwd) : tr('Own folder')}</Text>
                    <Icon name="chevron.down" size={10} weight="semibold" color="var(--secondary)" />
                  </Pill>
                </DropdownMenu>
              </ValueRow>
              {/* Of the saved folder, from a host that says (one with worktrees). */}
              {saved && saved.threadWorktrees !== null ? <GitStateLine bot={saved} /> : null}
            </VStack>
          )}
          <ApprovalRow draft={draft} catalog={models.catalog} onChange={onChange} />
          {/* Only from a host that has the box (its bots say). */}
          {saved && saved.maxThreads !== null ? (
            <ValueRow label="Threads at once">
              <Stepper label="Threads at once" value={draft.maxThreads ?? saved.maxThreads} min={1} max={MAX_THREADS} onChange={(count) => set('maxThreads', count)} />
            </ValueRow>
          ) : null}
          {/* Only where it matters (a project folder in a git repository: a bot's own folder keeps all its work in
              one place), from a host that has it and no projects. */}
          {!store.hasProjects && saved?.gitRepo && !saved.managedWorkspace && saved.threadWorktrees !== null ? (
            <SwitchRow
              title="Worktree per thread"
              detail="Threads that work at once in this repository each get their own branch."
              on={draft.threadWorktrees ?? saved.threadWorktrees}
              onChange={(on) => set('threadWorktrees', on)}
            />
          ) : null}
        </CardSection>

        <CardSection title="Activity" fold="bot.activity">
          <SwitchRow title="Notifications" detail="Get notified when this bot finishes or needs you" on={draft.notify ?? true} onChange={(on) => set('notify', on)} />
          {store.screen && !inspector ? (
            <SwitchRow
              title="Use the computer"
              detail={
                !controlReady(store.screen)
                  ? 'Let this bot use apps. On that computer, open Codync → Settings → Computer access to set up permissions first.'
                  : store.screen.enabled
                  ? 'Let this bot see the screen and use the mouse and keyboard. You can watch and take over from your phone.'
                  : store.screen.computerUse
                    ? 'Let this bot see the screen and use the mouse and keyboard.'
                    : "Let this bot see the screen and use the mouse and keyboard. On that computer, open Codync → Settings → Computer access to set up permissions first."
              }
              on={draft.computer ?? false}
              onChange={(on) => set('computer', on)}
            />
          ) : null}
          {/* Only from a host that has it (its bots say). */}
          {[...store.bots.values()].some((b) => b.manageTeam !== null) ? (
            <SwitchRow
              title="Manage the team"
              detail="Let this bot create bots and change any bot's setup, its own included. The bots it creates can too."
              on={draft.manageTeam ?? false}
              onChange={(on) => set('manageTeam', on)}
            />
          ) : null}
          <NetworkSwitches draft={draft} set={set} />
        </CardSection>

        <PluginToggles
          title="Connectors"
          empty="No connectors yet. Add GitHub, Linear, Notion and more from Extensions."
          items={connectors.map((c) => ({ id: c.id, name: c.name, detail: c.command ?? c.url ?? '' }))}
          selection={draft.connectors ?? []}
          onChange={(ids) => set('connectors', ids)}
        />
        <PluginToggles
          title="Skills"
          empty="No skills yet. Get some from Extensions, or write your own."
          items={skills.map((s) => ({ id: s.id, name: s.name, detail: '' }))}
          selection={draft.skills ?? []}
          onChange={(ids) => set('skills', ids)}
          onInfo={(item) => {
            const s = skills.find((x) => x.id === item.id)
            if (s) setViewingSkill({ id: s.id, name: s.name, description: s.description })
          }}
        />
        {error ? <Text size="footnote" tone="danger" className="fade-in">{error}</Text> : null}
      </div>
      <FormSheet open={viewingSkill !== null} onClose={() => setViewingSkill(null)}>
        {viewingSkill ? <SkillSheet key={viewingSkill.name} skill={viewingSkill} /> : null}
      </FormSheet>
      <Sheet open={pickingFolder} onClose={() => setPickingFolder(false)} width={460} height={520}>
        <FolderPicker
          start={(store.hasProjects ? draft.projects?.at(-1) : draft.cwd) || store.hello?.home || null}
          onPick={(path) => {
            if (store.hasProjects) onChange((d) => withProject(d, path))
            else set('cwd', path)
            setPickingFolder(false)
          }}
        />
      </Sheet>
    </div>
  )
}

/** A card of on/off switches for the connectors or skills installed on the computer. */
function PluginToggles({ title, empty, items, selection, onChange, onInfo }: {
  title: string
  empty: string
  items: { id: string; name: string; detail: string }[]
  selection: string[]
  onChange: (ids: string[]) => void
  /** Shows an item's details from an info button beside its switch. */
  onInfo?: (item: { id: string; name: string; detail: string }) => void
}) {
  const tr = useTr()
  return (
    <CardSection title={title}>
      {items.length ? null : <Text tone="secondary">{tr(empty)}</Text>}
      {items.map((item) => {
        const row = (
          <SwitchRow
            key={item.id}
            title={item.name}
            detail={item.detail || null}
            on={selection.includes(item.id)}
            onChange={(on) => onChange([...selection.filter((id) => id !== item.id), ...(on ? [item.id] : [])])}
          />
        )
        return onInfo ? (
          <HStack key={item.id} gap={4}>
            <div className="plugin-toggle">{row}</div>
            <IconButton title={tr('About {name}').replaceAll('{name}', () => item.name)} icon="info.circle" onClick={() => onInfo(item)} />
          </HStack>
        ) : row
      })}
    </CardSection>
  )
}

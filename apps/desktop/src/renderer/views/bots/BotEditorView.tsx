import { useCallback, useEffect, useRef, useState } from 'react'
import { fastModeRefusal, fastModeShown } from '@shared/fast-mode'
import { restLine, restOf, withBackend } from '@shared/fallbacks'
import { draftOf, type Bot, type BotDraft, type Personality } from '@shared/models'
import { AgentIcon } from '../../components/AgentIcon'
import { CharacterAvatar } from '../../components/Avatar'
import { ProactivePolicySection } from '../../components/ProactivePolicySection'
import { RobotAvatar, isRobotLook } from '../../components/RobotAvatar'
import { Button, CardSection, ChoicePicker, DropdownMenu, IconButton, Pill, Spinner, ValueRow } from '../../components/Controls'
import { FolderPicker } from '../../components/FolderPicker'
import { Icon } from '../../components/Icon'
import { ModalHeader, Sheet, SheetPage, useDismiss } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useStore } from '../../store/context'
import { useAgentModels } from './agent-models'
import { AgentAccountPicker } from './AgentAccountPicker'
import { AgentModelPicker } from './AgentModelPicker'
import { agentChoice, AgentStepBanner, FallbacksRow } from './Fallbacks'
import { useAgentSetup } from './agent-status'
import { lastPathComponent } from '../../lib/path'
import { errorText, isValid, normalized, useFollowSaved, usePlugins, type Plugins } from './drafts'
import { FormSheet } from '../marketplace/pieces'
import { SkillSheet, type SkillRef } from '../marketplace/SkillSheet'
import { AnchoredPanel, AutoTextArea, AvatarPicker, Field, SwitchRow } from './parts'
import { ApprovalRulesEditor } from './ApprovalRulesEditor'
import { SecurityProfilePicker } from './SecurityProfilePicker'
import { profileRules } from '@shared/security-profiles'
import { NetworkSwitches } from './NetworkSwitches'
import { ProactiveCheckIns } from './ProactiveCheckIns'
import './bots.css'
import { useTr } from '../../lib/use-language'


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

/**
 * Which settings a form shows: all of them (the editor sheet), what changes often
 * (instructions, model, fast mode: the inspector's Details tab) or the rest (its Settings tab).
 */
export type FormPart = 'all' | 'often' | 'rarely'

/** The desktop inspector next to a conversation: the same form, saved as you edit. */
export function BotSettingsPanel({ botId, part }: { botId: string; part: FormPart }) {
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

  return draft ? <BotSettingsForm draft={draft} onChange={change} error={error} plugins={plugins} part={part} /> : null
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

/** Bot settings, in sections in the Settings style (`CardSection`). */
function BotSettingsForm({ draft, onChange, error, plugins, part = 'all' }: {
  draft: BotDraft
  onChange: (update: (d: BotDraft) => BotDraft) => void
  error: string | null
  plugins: Plugins | null
  part?: FormPart
}) {
  const tr = useTr()
  const store = useStore()
  const setup = useAgentSetup()
  const [pickingAvatar, setPickingAvatar] = useState(false)
  const [pickingFolder, setPickingFolder] = useState(false)
  const [viewingSkill, setViewingSkill] = useState<SkillRef | null>(null)
  const avatarRef = useRef<HTMLButtonElement>(null)
  const set = <K extends keyof BotDraft>(key: K, value: BotDraft[K]) => onChange((d) => ({ ...d, [key]: value }))
  const setLayer = (layer: keyof Personality, value: string) =>
    onChange((d) => ({ ...d, personality: { soul: '', persona: '', heartbeat: '', ...d.personality, [layer]: value } }))
  // Options belong to one model.
  const setModel = useCallback((model: string | null) => onChange((d) => (d.model === model ? d : { ...d, model, modelOptions: {} })), [onChange])
  const setModelOptions = useCallback((modelOptions: Record<string, string>) => onChange((d) => ({ ...d, modelOptions })), [onChange])
  const models = useAgentModels(draft.backend, draft.account ?? null)
  const saved = draft.id ? (store.bots.get(draft.id) ?? null) : null

  const backends = store.hello?.backends ?? []
  const backend = backends.find((b) => b.id === draft.backend)
  const connectors = plugins?.connectors ?? []
  const skills = plugins?.skills ?? []

  // The agent's own models; a model that is set but not listed (an older host, another agent) stays pickable.
  const backgroundChoices = [
    { id: '', label: 'The bot\'s own model' },
    ...(models.catalog?.models ?? []).map((m) => ({ id: m.id, label: m.name })),
    ...(draft.backgroundModel && !models.catalog?.models.some((m) => m.id === draft.backgroundModel)
      ? [{ id: draft.backgroundModel, label: draft.backgroundModel }]
      : []),
  ]
  const loadProfiles = useCallback(() => store.securityProfiles(), [store])
  const instructions = [
      <Field key="standing" label="Standing instructions" detail="Rules that always apply. Put task-specific requests in the chat instead.">
        <AutoTextArea className="field-box" value={draft.description} placeholder="e.g. Reviews PRs. Never pushes without asking." minRows={3} maxRows={8} onChange={(v) => set('description', v)} />
      </Field>,
      <Field key="check" label="Check before done" detail="A command run in the workspace after each turn. If it fails, the turn doesn't count as done.">
        <AutoTextArea className="field-box" value={draft.verifyCommand ?? ''} placeholder="e.g. npm test" minRows={1} maxRows={3} onChange={(v) => set('verifyCommand', v)} />
      </Field>,
      <Field key="soul" label="Soul" detail="Values and boundaries that never change. Written by you; the bot can't edit it.">
        <AutoTextArea className="field-box" value={draft.personality?.soul ?? ''} placeholder="e.g. Honest, careful with other people's data." minRows={2} maxRows={8} onChange={(v) => setLayer('soul', v)} />
      </Field>,
      <Field key="persona" label="Persona" detail="Its voice and manner.">
        <AutoTextArea className="field-box" value={draft.personality?.persona ?? ''} placeholder="e.g. Dry and brief. No emoji." minRows={2} maxRows={8} onChange={(v) => setLayer('persona', v)} />
      </Field>,
      <Field key="heartbeat" label="Heartbeat" detail="What it does when it checks in on its own.">
        <AutoTextArea className="field-box" value={draft.personality?.heartbeat ?? ''} placeholder="e.g. Look for failing tests and say only if something broke." minRows={2} maxRows={8} onChange={(v) => setLayer('heartbeat', v)} />
      </Field>,
      <Field key="background" label="Background model" detail="A cheaper model for routines and check-ins, where nobody is waiting.">
        <ChoicePicker
          selection={draft.backgroundModel ?? ''}
          options={backgroundChoices}
          onChange={(id) => set('backgroundModel', id || null)}
        />
      </Field>,
      draft.backgroundModel ? (
        <SwitchRow
          key="saver"
          title="Saver"
          detail="At 80 percent of this bot's daily token budget, new sessions start on the background model until tomorrow."
          on={draft.saver ?? false}
          onChange={(on) => set('saver', on)}
        />
      ) : null,
    ]
  const ownRest = saved ? restOf(saved, draft.backend, Date.now()) : null
  // An account (its own agent sign-in) belongs to one agent, so another agent starts on the default one.
  const account = (
    <AgentAccountPicker key="account" backend={draft.backend} selection={draft.account} online={store.connection.kind === 'online'}
      contextKey={JSON.stringify([store.contextId, store.computer.id, draft.id ?? 'new'])} load={() => store.agentAccounts(draft.backend)}
      supported={backend?.accounts === true}
      authenticate={() => store.authenticateAgentAccount(draft.backend)}
      remove={(id) => store.deleteAgentAccount(id)}
      signOut={backend?.signOut ? () => store.signOutAgent(draft.backend) : undefined}
      onChange={(id) => onChange((d) => (d.account === id ? d : { ...d, account: id, model: null, modelOptions: {} }))} />
  )
  const agent = (
    <VStack key="agent" gap={8}>
      <AgentModelPicker backend={draft.backend} models={models} selection={draft.model}
        onChange={setModel} options={draft.modelOptions ?? {}} onOptionsChange={setModelOptions}
        agents={[...backends.map(agentChoice), { id: 'custom', label: 'Custom command', leading: <AgentIcon registry="custom" size={14} /> }]}
        onAgentChange={(id) => onChange((d) => (d.backend === id ? d : { ...withBackend(d, id), account: null }))} fast={draft.fastMode ?? false}
        fastControl={((draft.backend !== 'custom' && models.catalog?.fastMode) || draft.fastMode || (saved?.backend === draft.backend && fastModeShown(saved))) ?
          <SwitchRow title="Fast mode" detail={saved ? fastModeRefusal(saved) : null} on={draft.fastMode ?? false} onChange={(on) => set('fastMode', on)} /> : null}>
        {ownRest && <Text size="caption" tone="secondary">{restLine(ownRest, Date.now())}</Text>}
        {backend && !backend.available && <Text size="caption" tone="attention">{backend.installHint}</Text>}
        {draft.backend === 'custom' && <input className="field-box bot-command" value={draft.command ?? ''} placeholder="ACP command, e.g. my-agent --acp" spellCheck={false} onChange={(e) => set('command', e.target.value || null)} />}
      </AgentModelPicker>
      {backend && backend.id !== 'custom' && backend.installed && backend.signedIn !== true ? (
        <HStack gap={8} className="agent-signin-row fade-in">
          <Text size="caption" tone="secondary" grow>{backend.signedIn === false ? tr('Not signed in.') : tr("We can't tell the sign-in yet.")}</Text>
          <Button kind="secondary" onClick={() => setup.open(backend.id)}>{backend.signedIn === false ? 'Sign in' : 'Check'}</Button>
        </HStack>
      ) : null}
      {account}
    </VStack>
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

  if (part === 'often') {
    return (
      <div className="bot-form">
        <div className="bot-form-content">
          <CardSection title="Instructions">{instructions}</CardSection>
          <CardSection title="Agent">
            {step}
            {agent}
            {fallbacks}
          </CardSection>
          {error ? <Text size="footnote" tone="danger" className="fade-in">{error}</Text> : null}
        </div>
        {setup.sheet}
      </div>
    )
  }

  return (
    <div className="bot-form">
      <div className="bot-form-content">
        <CardSection title="Profile">
          <ValueRow label="Avatar">
            <button ref={avatarRef} className="avatar-button small" title="Edit Bot avatar" aria-label="Edit Bot avatar" onClick={() => setPickingAvatar((p) => !p)}>
              {isRobotLook(draft.avatarLook) ? (
                <RobotAvatar look={draft.avatarLook!} color={draft.avatarColor} size={24} />
              ) : (
                <CharacterAvatar shape={draft.avatarShape} color={draft.avatarColor} size={24} />
              )}
            </button>
            <AnchoredPanel open={pickingAvatar} onClose={() => setPickingAvatar(false)} anchor={avatarRef}>
              <div className="avatar-picker-panel">
                <AvatarPicker shape={draft.avatarShape} color={draft.avatarColor} onShape={(s) => set('avatarShape', s)} onColor={(c) => set('avatarColor', c)} />
              </div>
            </AnchoredPanel>
          </ValueRow>
          <Field label="Name">
            <input className="field-box" value={draft.name} placeholder="Name" onChange={(e) => set('name', e.target.value)} />
          </Field>
          {part === 'all' ? instructions : null}
          {/* Only a row when a bot made it: CardSection gives every child a row. */}
          {saved?.createdBy ? <CreatedBy bot={saved} creator={saved.createdBy} /> : null}
        </CardSection>

        {/* In the inspector the agent and its model are on the Details tab; what's left is where it works. */}
        <CardSection title={part === 'all' ? 'Agent' : 'Workspace'}>
          {part === 'all' ? step : null}
          <ValueRow label="Computer">
            <Text truncate>{store.hostName}</Text>
          </ValueRow>
          {part === 'all' ? agent : null}
          {part === 'all' ? fallbacks : null}
          <ValueRow
            label="Workspace"
            detail={
              draft.cwd
                ? 'This project is the default starting folder. The bot can work elsewhere when you ask.'
                : 'This bot has its own space for files. It can work in other folders when you ask.'
            }
          >
            <DropdownMenu
              title={draft.cwd || 'A persistent workspace allocated for this bot'}
              items={() => [
                { title: 'Personal workspace', selected: !draft.cwd, action: () => set('cwd', '') },
                { title: 'Choose project folder…', selected: !!draft.cwd, action: () => setPickingFolder(true) },
              ]}
            >
              <Pill outlined className="workspace-pill">
                <Text truncate className="workspace-pill-label">{draft.cwd ? lastPathComponent(draft.cwd) : tr(`Personal`)}</Text>
                <Icon name="chevron.down" size={10} weight="semibold" color="var(--secondary)" />
              </Pill>
            </DropdownMenu>
          </ValueRow>
          <SecurityProfilePicker
            selection={draft.securityProfile ?? null}
            saved={saved?.securityProfile ?? null}
            load={loadProfiles}
            onChange={(profile) =>
              onChange((d) => ({
                ...d,
                securityProfile: profile?.id ?? null,
                // The form shows what the profile will write; the host writes it on save.
                ...(profile ? { permission: profile.permission, approvalRules: profileRules(profile) } : {}),
              }))
            }
          />
          <ValueRow
            label="Permissions"
            detail={
              draft.permission === 'auto'
                ? "Tool requests are approved automatically. Your agent's own settings (like Claude Code's permission rules) still apply."
                : "You'll get an approval card and a notification whenever the agent asks."
            }
          >
            <ChoicePicker
              selection={draft.permission}
              options={[
                { id: 'ask', label: 'Ask me' },
                { id: 'auto', label: 'Approve automatically' },
              ]}
              onChange={(id) => set('permission', id)}
            />
          </ValueRow>
          <ApprovalRulesEditor
            rules={draft.approvalRules ?? []}
            onChange={(rules) => set('approvalRules', rules)}
            test={(command, rules) =>
              draft.id
                ? store.testApprovalRule(draft.id, command, rules, draft.actionRulesOn === undefined ? undefined : { on: draft.actionRulesOn, policies: draft.actionRules ?? {} })
                : Promise.resolve(null)
            }
            quick={draft.actionRulesOn === undefined ? undefined : { on: draft.actionRulesOn, policies: draft.actionRules ?? {} }}
            onQuick={(next) => onChange((d) => ({ ...d, actionRulesOn: next.on, actionRules: next.policies }))}
          />
        </CardSection>

        <CardSection title="Activity">
          <SwitchRow title="Notifications" detail="Get notified when this bot finishes or needs you" on={draft.notify ?? true} onChange={(on) => set('notify', on)} />
          <SwitchRow
            title="Chief of Staff (Coordinator)"
            detail="Supervises stalled or failing bots in teams and groups, with automatic triage and up to 2 retries per incident."
            on={draft.isChief ?? false}
            onChange={(on) => set('isChief', on)}
          />
          {store.screen && part === 'all' ? (
            <SwitchRow
              title="Use the computer"
              detail={
                store.screen.platform !== 'windows' && !(store.screen.enabled && store.screen.connected && store.screen.capture && store.screen.input)
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

        <ProactivePolicySection
          avatarColorId={draft.avatarColor}
          avatarLook={draft.avatarLook ?? undefined}
          onChangeLook={(look) => set('avatarLook', look ?? null)}
          onApplyRoleTemplate={(tpl) => onChange((d) => ({ ...d, description: draft.description?.trim() ? draft.description : tpl.standingInstructions, avatarLook: tpl.avatarLook, avatarColor: tpl.avatarColor }))}
        />

        <ProactiveCheckIns settings={draft.proactive} onChange={(next) => set('proactive', next)} />

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
          items={skills.map((s) => ({ id: s.id, name: s.name, detail: s.description }))}
          selection={draft.skills ?? []}
          onChange={(ids) => set('skills', ids)}
          onInfo={(s) => setViewingSkill({ id: s.id, name: s.name, description: s.detail })}
        />
        {error ? <Text size="footnote" tone="danger" className="fade-in">{error}</Text> : null}
      </div>
      <FormSheet open={viewingSkill !== null} onClose={() => setViewingSkill(null)}>
        {viewingSkill ? <SkillSheet key={viewingSkill.name} skill={viewingSkill} /> : null}
      </FormSheet>
      <Sheet open={pickingFolder} onClose={() => setPickingFolder(false)} width={460} height={520}>
        <FolderPicker
          start={draft.cwd || store.hello?.home || null}
          onPick={(path) => {
            set('cwd', path)
            setPickingFolder(false)
          }}
        />
      </Sheet>
      {setup.sheet}
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
            <IconButton title={`About ${item.name}`} icon="info.circle" onClick={() => onInfo(item)} />
          </HStack>
        ) : row
      })}
    </CardSection>
  )
}

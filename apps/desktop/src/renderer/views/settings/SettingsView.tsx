import { Fragment, useEffect, useState } from 'react'
import type { Screen } from '@shared/analytics'
import { APPEARANCES, type Appearance } from '@shared/appearance'
import { islandLabel } from '@shared/island'
import { VOICE_SHORTCUTS, type VoiceShortcut } from '@shared/voice-shortcut'
import { Button, CardForm, CardSection, ChoicePicker, IconButton, ProgressBar, Switch, ValueRow } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { useDismiss } from '../../components/Overlay'
import { HStack, IconSlot, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useModel } from '../../lib/observable'
import { appLanguages, languageTitle } from '../../lib/l10n'
import { useTr } from '../../lib/use-language'
import { DEFAULT_TEXT_SIZE, prefs, TEXT_SIZES, usePref } from '../../lib/prefs'
import { useAccount } from '../../store/account'
import { useScreenViewed } from '../../store/analytics'
import { StoreContext, useApp } from '../../store/context'
import { ComputerBadge } from '../ComputerBadge'
import { UpdateButton, useUpdates } from '../UpdateButton'
import { ComputersView } from './ComputersView'
import { errorText, isMac, thisMac } from './parts'
import { UsageLimits } from './UsageLimits'
import { VoiceChatSettingsView } from './VoiceChatSettingsView'
import { ComputerAccessView } from '../computer-access/ComputerAccessView'
import { FormSheet } from '../marketplace/pieces'
import { AnalyticsSheet } from './AnalyticsSheet'
import { HiddenBotsSection } from './HiddenBotsSection'

/** The chat window's Settings: pages in a sidebar, ChatGPT's settings layout. */
export type SettingsPage = 'general' | 'computers' | 'access' | 'voice' | 'usage' | 'updates'

const thisComputer = isMac ? 'This Mac' : 'This computer'
const pages: { id: SettingsPage; title: string; label?: string; group: string; icon: string }[] = [
  { id: 'general', title: 'General', group: 'Personal', icon: 'gearshape' },
  { id: 'computers', title: 'Computers & devices', label: 'Computers', group: thisComputer, icon: 'desktopcomputer' },
  { id: 'access', title: 'Computer access', group: thisComputer, icon: 'hand.raised' },
  { id: 'voice', title: 'Voice chat', group: 'Personal', icon: 'waveform' },
  { id: 'usage', title: 'Usage', group: 'Personal', icon: 'gauge.with.dots.needle.33percent' },
  { id: 'updates', title: 'Updates', group: thisComputer, icon: 'arrow.down.circle' },
]

/** Each page as `screen_viewed` names it; Computer access and Voice chat report themselves. */
const pageScreens: Record<SettingsPage, Screen | null> = { general: 'settings', updates: 'settings', computers: 'computers', usage: 'usage', access: null, voice: null }

export function SettingsView({ page: initial }: { page: SettingsPage }) {
  // Kept here: the sheet's content doesn't follow the window's state once shown.
  const [page, setPage] = useState(initial)
  const app = useApp()
  const dismiss = useDismiss()
  const tr = useTr()
  const current = pages.find((p) => p.id === page)!
  useScreenViewed(pageScreens[page])
  return (
    <div className="settings">
      <nav className="settings-sidebar">
        {['Personal', thisComputer].map((group) => (
          <Fragment key={group}>
            <div className="settings-group">{tr(group)}</div>
            {pages.filter((p) => p.group === group).map((item) => (
              <button key={item.id} className={`settings-item ${item.id === page ? 'selected' : ''}`} aria-current={item.id === page ? 'page' : undefined} onClick={() => setPage(item.id)}>
                <IconSlot width={18} tone="secondary">
                  <Icon name={item.icon} size={13} />
                </IconSlot>
                <Text truncate grow>{tr(item.label ?? item.title)}</Text>
                {item.id === 'computers' && app.approvals.length > 0 ? (
                  <Text size="caption" weight="semibold" tone="secondary">{app.approvals.length}</Text>
                ) : null}
              </button>
            ))}
          </Fragment>
        ))}
      </nav>
      <div className="settings-divider" />
      <div className="settings-main">
        <HStack gap={12} className="settings-title">
          <Icon name={current.icon} size={30} weight="medium" />
          <Text as="h2" size={24} weight="semibold" tone="text">{tr(current.title)}</Text>
        </HStack>
        <div key={page} className="settings-page">
          <PageContent page={page} />
        </div>
      </div>
      <span className="settings-close">
        <IconButton title="Close" icon="xmark" onClick={dismiss} />
      </span>
    </div>
  )
}

function PageContent({ page }: { page: SettingsPage }) {
  const tr = useTr()
  const app = useApp()
  switch (page) {
    case 'access':
      return <ComputerAccessView />
    case 'general':
      return <GeneralSettings />
    case 'computers':
      return <ComputersView />
    case 'voice':
      return app.local ? (
        <StoreContext.Provider value={app.local}>
          <VoiceChatSettingsView showsHeader={false} />
        </StoreContext.Provider>
      ) : (
        <CardForm>
          <Text tone="secondary">{tr(isMac ? "Voice keys are kept by This Mac's host, which isn't running." : `Voice keys are kept by ${thisMac}'s host, which isn't running.`)}</Text>
        </CardForm>
      )
    case 'usage':
      return (
        <CardForm>
          <UsageLimitsList />
        </CardForm>
      )
    case 'updates':
      return <UpdateSettings />
  }
}

/** Account, window and app preferences. */
function GeneralSettings() {
  const account = useAccount()
  const [appearance, setAppearance] = useState<Appearance>('system')
  useEffect(() => void window.codync.app.appearance().then(setAppearance), [])
  const [textSize, setTextSize] = usePref(prefs.textSize)
  const [language, setLanguage] = usePref(prefs.appLanguage)
  const systemLanguage = typeof navigator === 'undefined' ? 'en' : navigator.language
  const [usageIconStyle, setUsageIconStyle] = usePref(prefs.usageIconStyle)
  const [compact, setCompact] = usePref(prefs.sidebarCompact)
  const [showInDock, setShowInDock] = useState(true)
  useEffect(() => void window.codync.app.showInDock().then(setShowInDock), [])
  const [launchAtLogin, setLaunchAtLogin] = useState(false)
  useEffect(() => void window.codync.app.launchAtLogin().then(setLaunchAtLogin), [])
  const [island, setIsland] = useState(false)
  useEffect(() => void window.codync.island.enabled().then(setIsland), [])
  const [voiceShortcut, setVoiceShortcut] = useState<VoiceShortcut>('off')
  useEffect(() => void window.codync.speech.shortcut().then(setVoiceShortcut), [])
  const tr = useTr()

  return (
    <CardForm>
      <CardSection title="Account" footer={account.errorMessage}>
        <div className="settings-row">
          <Icon name="person.crop.circle" size={26} color="var(--secondary)" />
          <VStack grow className="settings-stack">
            <Text truncate>
              {account.isSignedIn ? (account.user?.email ?? tr(`Signed in`)) : tr(`Not signed in`)}
            </Text>
            <Text size="caption" tone="secondary">
              {account.isSignedIn ? tr(`Reach your computers from anywhere`) : tr(`Sign in to reach your computers from anywhere`)}
            </Text>
          </VStack>
          {account.isSignedIn ? (
            <Button kind="secondary" onClick={() => void account.signOut()} disabled={account.isBusy}>
              {account.isBusy ? 'Please wait…' : 'Sign out'}
            </Button>
          ) : (
            <>
              <Button kind="secondary" onClick={() => void account.signIn('apple')} disabled={account.isBusy}>
                Apple
              </Button>
              <Button kind="secondary" onClick={() => void account.signIn('google')} disabled={account.isBusy}>
                Google
              </Button>
            </>
          )}
        </div>
      </CardSection>
      <CardSection title="Appearance">
        <ValueRow label="Theme">
          <ChoicePicker selection={appearance} options={[...APPEARANCES]} onChange={(value) => void window.codync.app.setAppearance(value).then(setAppearance)} />
        </ValueRow>
        <ValueRow label="Text size">
          <ChoicePicker
            selection={textSize}
            options={TEXT_SIZES.map((s) => ({ id: s, label: s === DEFAULT_TEXT_SIZE ? tr('{size} pt (Default)').replace('{size}', String(s)) : `${s} pt` }))}
            onChange={setTextSize}
          />
        </ValueRow>
        <ValueRow label="Usage icons">
          <ChoicePicker selection={usageIconStyle} options={[{ id: 'character', label: 'Character' }, { id: 'original', label: 'Original' }]} onChange={setUsageIconStyle} />
        </ValueRow>
        <Switch on={compact} onChange={setCompact}>
          Compact sidebar
        </Switch>
      </CardSection>
      <CardSection
        title="Language"
        footer="Choose the language for this device. Unsupported system languages use English."
      >
        <ValueRow label="App language">
          <ChoicePicker
            selection={language}
            options={appLanguages.map((id) => ({
              id,
              label: languageTitle(id, language, systemLanguage),
            }))}
            onChange={setLanguage}
          />
        </ValueRow>
      </CardSection>
      <CardSection title="System">
        {window.codync.platform === 'darwin' ? <Switch on={showInDock} onChange={(on) => void window.codync.app.setShowInDock(on).then(setShowInDock)}>Show in Dock</Switch> : null}
        {window.codync.island.supported ? (
          <Switch on={island} onChange={(on) => void window.codync.island.setEnabled(on).then(setIsland)}>
            {islandLabel(window.codync.platform)}
          </Switch>
        ) : null}
        {/* On-device speech, so macOS only. */}
        {window.codync.speech.available ? (
          <ValueRow label="Voice input shortcut" detail="Talk to a bot from any app, then tap the key once more to send.">
            <ChoicePicker selection={voiceShortcut} options={VOICE_SHORTCUTS} onChange={(s) => void window.codync.speech.setShortcut(s).then(setVoiceShortcut)} />
          </ValueRow>
        ) : null}
        <Switch on={launchAtLogin} onChange={(on) => void window.codync.app.setLaunchAtLogin(on).then(setLaunchAtLogin)}>
          Open at login
        </Switch>
      </CardSection>
      <PrivacySettings />
      <HiddenBotsSection />
    </CardForm>
  )
}

/** Product analytics on this computer's host; only the computer itself may change it. */
function PrivacySettings() {
  const app = useApp()
  const local = useModel(app.local)
  const running = local !== null && local.connection.kind === 'online'
  const [about, setAbout] = useState(false)
  const tr = useTr()
  const setShared = (on: boolean) => void local?.setAnalytics(on).catch((e: unknown) => app.setError(errorText(e)))
  return (
    <>
      <CardSection title="Privacy" footer="Which features you use, never your messages, code or files. Helps us spot bugs sooner and make Codync better for you.">
        <HStack gap={4}>
          <span className={running ? undefined : 'settings-off'}>{tr('Share usage analytics')}</span>
          <IconButton title="About usage analytics" icon="info.circle" onClick={() => setAbout(true)} />
          <Switch on={local?.analytics === true} onChange={setShared} disabled={!running} label="Share usage analytics" className="settings-privacy-switch" />
        </HStack>
      </CardSection>
      <FormSheet open={about} onClose={() => setAbout(false)}>
        <AnalyticsSheet />
      </FormSheet>
    </>
  )
}

/** Every computer's usage limits, grouped by computer when there's more than one. */
function UsageLimitsList() {
  const tr = useTr()
  const app = useApp()
  const stores = app.computers.flatMap((c) => app.store(c.id) ?? [])
  const withUsage = stores.filter((s) => s.usage.providers.length > 0)
  if (withUsage.length === 0) return <Text tone="secondary">{tr(`No usage information yet.`)}</Text>
  return (
    <>
      {withUsage.flatMap((store) => [
        stores.length > 1 ? (
          <HStack key={`${store.computer.id}-label`} gap={6} className="settings-usage-computer">
            <ComputerBadge computer={store.computer} size={16} />
            {store.hostName}
          </HStack>
        ) : null,
        <UsageLimits key={store.computer.id} usage={store.usage} />,
      ])}
    </>
  )
}

/** The app's update check and preferences, the same as the menu bar's Updates menu. */
function UpdateSettings() {
  const updates = useUpdates()
  if (!updates) return null
  const footer = updates.supported ? null : 'Updates are available in release builds.'
  const lastChecked = updates.lastCheck
    ? `Last checked ${new Date(updates.lastCheck).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })}`
    : null
  return (
    <CardForm>
      <CardSection title="Codync" footer={footer}>
        {[
          <ValueRow key="version" label="Version" detail={lastChecked}>
            <HStack as="span" gap={10}>
              <span>{window.codync.appVersion}</span>
              <UpdateButton updates={updates} />
            </HStack>
          </ValueRow>,
          ...(updates.downloadPercent !== null
            ? [
                <div key="progress" className="settings-row settings-appear">
                  <ProgressBar value={updates.downloadPercent / 100} label={`Downloading Codync ${updates.availableVersion ?? ''}`} />
                </div>,
              ]
            : []),
          <Switch key="check" on={updates.autoCheck} onChange={(on) => window.codync.updates.setAutoCheck(on)} disabled={!updates.supported}>
            Automatically check for updates
          </Switch>,
          <Switch key="download" on={updates.autoDownload} onChange={(on) => window.codync.updates.setAutoDownload(on)} disabled={!updates.supported}>
            Automatically download and install
          </Switch>,
          ...(updates.error
            ? [
                <div key="error" className="settings-row settings-appear">
                  <Text tone="attention" grow>{updates.error}</Text>
                  <Button kind="secondary" onClick={() => window.codync.updates.check()} disabled={updates.checking}>
                    Retry
                  </Button>
                </div>,
              ]
            : []),
        ]}
      </CardSection>
    </CardForm>
  )
}

import { useEffect, useState } from 'react'
import type { UpdateState } from '@shared/ipc'
import { Button, CardForm, CardSection, ChoicePicker, IconButton, Switch, ValueRow } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { useDismiss } from '../../components/Overlay'
import { font } from '../../lib/fonts'
import { useModel } from '../../lib/observable'
import { DEFAULT_TEXT_SIZE, prefs, TEXT_SIZES, usePref } from '../../lib/prefs'
import { useAccount } from '../../store/account'
import { StoreContext, useApp } from '../../store/context'
import { ComputerBadge } from '../ComputerBadge'
import { ComputersView } from './ComputersView'
import { errorText, isMac, thisMac } from './parts'
import { UsageLimits } from './UsageLimits'
import { VoiceChatSettingsView } from './VoiceChatSettingsView'
import { ComputerAccessView } from '../computer-access/ComputerAccessView'

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

export function SettingsView({ page: initial }: { page: SettingsPage }) {
  // Kept here: the sheet's content doesn't follow the window's state once shown.
  const [page, setPage] = useState(initial)
  const app = useApp()
  const dismiss = useDismiss()
  const current = pages.find((p) => p.id === page)!
  return (
    <div className="settings">
      <nav className="settings-sidebar">
        {['Personal', thisComputer].map((group) => (
          <div key={group} style={{ display: 'contents' }}>
            <div className="settings-group" style={{ paddingTop: group === 'Personal' ? 0 : 16 }}>{group}</div>
            {pages.filter((p) => p.group === group).map((item) => (
              <button key={item.id} className={`settings-item ${item.id === page ? 'selected' : ''}`} aria-selected={item.id === page} onClick={() => setPage(item.id)}>
                <span style={{ width: 18, display: 'flex', justifyContent: 'center', color: 'var(--secondary)' }}>
                  <Icon name={item.icon} size={13} />
                </span>
                <span style={{ flex: 1, minWidth: 0, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{item.label ?? item.title}</span>
                {item.id === 'computers' && app.approvals.length > 0 ? (
                  <span style={{ ...font(11, 'semibold'), color: 'var(--secondary)' }}>{app.approvals.length}</span>
                ) : null}
              </button>
            ))}
          </div>
        ))}
      </nav>
      <div className="settings-divider" />
      <div className="settings-main">
        <div style={{ ...font(24, 'semibold'), color: 'var(--text)', padding: '30px 32px 8px' }}>{current.title}</div>
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
          <span style={{ color: 'var(--secondary)' }}>Voice keys are kept by {thisMac}'s host, which isn't running.</span>
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
  const [textSize, setTextSize] = usePref(prefs.textSize)
  const [usageIconStyle, setUsageIconStyle] = usePref(prefs.usageIconStyle)
  const [compact, setCompact] = usePref(prefs.sidebarCompact)
  const [showInDock, setShowInDock] = useState(true)
  useEffect(() => void window.codync.app.showInDock().then(setShowInDock), [])
  const [launchAtLogin, setLaunchAtLogin] = useState(false)
  useEffect(() => void window.codync.app.launchAtLogin().then(setLaunchAtLogin), [])

  return (
    <CardForm>
      <CardSection title="Account" footer={account.errorMessage}>
        <div className="settings-row">
          <Icon name="person.crop.circle" size={26} color="var(--secondary)" />
          <div className="settings-stack" style={{ flex: 1 }}>
            <span style={{ whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
              {account.isSignedIn ? (account.user?.email ?? 'Signed in') : 'Not signed in'}
            </span>
            <span style={{ ...font('caption'), color: 'var(--secondary)' }}>
              {account.isSignedIn ? 'Reach your computers from anywhere' : 'Sign in to reach your computers from anywhere'}
            </span>
          </div>
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
        <ValueRow label="Text size">
          <ChoicePicker
            selection={textSize}
            options={TEXT_SIZES.map((s) => ({ id: s, label: s === DEFAULT_TEXT_SIZE ? `${s} pt (Default)` : `${s} pt` }))}
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
      <CardSection title="System">
        {window.codync.platform === 'darwin' ? <Switch on={showInDock} onChange={(on) => void window.codync.app.setShowInDock(on).then(setShowInDock)}>Show in Dock</Switch> : null}
        <Switch on={launchAtLogin} onChange={(on) => void window.codync.app.setLaunchAtLogin(on).then(setLaunchAtLogin)}>
          Open at login
        </Switch>
      </CardSection>
      <PrivacySettings />
    </CardForm>
  )
}

/** Product analytics on this computer's host; only the computer itself may change it. */
function PrivacySettings() {
  const app = useApp()
  const local = useModel(app.local)
  const running = local !== null && local.connection.kind === 'online'
  const setShared = (on: boolean) => void local?.setAnalytics(on).catch((e: unknown) => app.setError(errorText(e)))
  return (
    <CardSection title="Privacy" footer="Which features you use, never your messages, code or files. Private, used only to improve Codync.">
      <Switch on={local?.analytics === true} onChange={setShared} disabled={!running}>
        Share usage analytics
      </Switch>
    </CardSection>
  )
}

/** Every computer's usage limits, grouped by computer when there's more than one. */
function UsageLimitsList() {
  const app = useApp()
  const stores = app.computers.flatMap((c) => app.store(c.id) ?? [])
  const withUsage = stores.filter((s) => s.usage.providers.length > 0)
  if (withUsage.length === 0) return <span style={{ color: 'var(--secondary)' }}>No usage information yet.</span>
  return (
    <>
      {withUsage.flatMap((store) => [
        stores.length > 1 ? (
          <div key={`${store.computer.id}-label`} style={{ ...font('headline'), display: 'flex', alignItems: 'center', gap: 6 }}>
            <ComputerBadge computer={store.computer} size={16} />
            {store.hostName}
          </div>
        ) : null,
        <UsageLimits key={store.computer.id} usage={store.usage} />,
      ])}
    </>
  )
}

/** The app's update check and preferences, the same as the menu bar's Updates menu. */
function UpdateSettings() {
  const [updates, setUpdates] = useState<UpdateState | null>(null)
  useEffect(() => {
    void window.codync.updates.state().then(setUpdates)
    return window.codync.updates.onChange(setUpdates)
  }, [])
  if (!updates) return null
  const footer = !updates.supported
    ? 'Updates are available in release builds.'
    : updates.waitingForApp
      ? `Waiting for iPhone app ${updates.waitingForApp} to pass App Store review.`
      : null
  const lastChecked = updates.lastCheck
    ? `Last checked ${new Date(updates.lastCheck).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })}`
    : null
  return (
    <CardForm>
      <CardSection title="Codync" footer={footer}>
        {[
          <ValueRow key="version" label="Version" detail={lastChecked}>
            <span style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
              <span>{window.codync.appVersion}</span>
              <Button kind="secondary" onClick={() => window.codync.updates.check()} disabled={!updates.canCheck && !updates.staged}>
                {updates.availableVersion ? `Update to ${updates.availableVersion}` : 'Check for updates'}
              </Button>
            </span>
          </ValueRow>,
          <Switch key="check" on={updates.autoCheck} onChange={(on) => window.codync.updates.setAutoCheck(on)} disabled={!updates.supported}>
            Automatically check for updates
          </Switch>,
          <Switch key="download" on={updates.autoDownload} onChange={(on) => window.codync.updates.setAutoDownload(on)} disabled={!updates.supported}>
            Automatically download and install
          </Switch>,
          ...(updates.error
            ? [
                <div key="error" className="settings-row settings-appear">
                  <span style={{ flex: 1, color: 'var(--warning)' }}>{updates.error}</span>
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

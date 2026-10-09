import type { ReactNode } from 'react'
import { Button, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { useComputerAccess } from './use-computer-access'
import appIcon from '../../../../resources/icon.png'
import './computer-access.css'

/** Shared by onboarding and Settings. Permission requests remain explicit and local. */
export function ComputerAccessView({ onContinue }: { onContinue?: (ready: boolean) => void }) {
  const access = useComputerAccess()
  const { screen, step, busy } = access
  const mac = window.codync.platform === 'darwin'
  const windows = window.codync.platform === 'win32'
  const ready = step === 'ready'
  const canRequest = ['capture', 'input', 'portal', 'ready'].includes(step)
  const helperReady = screen?.enabled === true && screen.connected && step !== 'background'
  const completed = windows ? Number(ready) : Number(helperReady) + (mac
    ? Number(screen?.capture === true) + Number(screen?.input === true)
    : Number(screen?.capture === true && screen.input))
  const request = (permission: 'capture' | 'input' | 'portal', title: string) => (
    <Button kind={step === permission ? 'primary' : 'secondary'} disabled={!canRequest || busy !== null} onClick={() => void access.request(permission)}>
      {busy === permission ? <><Spinner /> Waiting for permission…</> : title}
    </Button>
  )
  return (
    <section className={`computer-access ${onContinue ? 'onboarding' : ''}`} aria-label="Computer access setup">
      <div className="computer-access-content">
        <header className="computer-access-header">
          <div className="computer-access-identity">
            <img src={appIcon} width={40} height={40} alt="Codync" />
            <div><span>This computer</span><strong>{window.codync.computerName || 'Your computer'}</strong></div>
          </div>
          <h1>{ready ? 'Your computer is ready' : 'Computer access'}</h1>
          <p>{windows ? 'Let a bot work in your apps. You choose which bots can use this computer.' : 'Work in your apps with a bot, or access your screen from your phone. You choose which bots can use this computer.'}</p>
        </header>
        {step === 'host' ? <AccessNotice title="Waiting for your computer" icon="desktopcomputer">
          Start or restart the host from Codync’s menu to continue. We’ll check again automatically.
        </AccessNotice> : null}
        {step === 'unavailable' ? <AccessNotice title="Codync Screen is missing" icon="desktopcomputer.trianglebadge.exclamationmark">
          Install the full Codync desktop app to finish setup.
        </AccessNotice> : null}
        {access.error ? <AccessNotice title="Couldn’t finish setup" icon="exclamationmark.circle" error>{access.error}</AccessNotice> : null}
        <div className="computer-access-section-label"><span>{windows ? 'Availability' : 'Permissions'}</span><span>{ready ? 'All set' : `${completed} of ${mac ? 3 : windows ? 1 : 2} ready`}</span></div>
        <div className="computer-access-steps">
          {windows ? <AccessRow title="Computer control" icon="cursorarrow.motionlines" detail="Bots can use apps on this computer. Remote screen viewing from your phone isn’t available on Windows yet." done={ready} /> : <>
            <AccessRow title="Codync Screen" icon="desktopcomputer" detail="Runs in the background to share your screen and connect your controls." done={helperReady} active={['enable', 'helper', 'background'].includes(step)} status={step === 'background' ? 'Needs approval' : undefined}>
              {['enable', 'helper'].includes(step) ? <Button disabled={busy !== null} onClick={() => void access.enable()}>{busy === 'enable' ? <><Spinner /> Starting…</> : step === 'helper' ? 'Start again' : 'Enable Codync Screen'}</Button> : null}
              {step === 'background' ? <><p className="computer-access-inline-note">Allow Codync to run in the background in Login Items.</p><Button onClick={() => access.openSettings('background')}>Open Login Items <Icon name="arrow.up.right.square" size={10} /></Button></> : null}
            </AccessRow>
            {mac ? <>
              <AccessRow title="Screen recording" icon="record.circle" detail="Lets Codync see your screen, including anything visible in your apps." done={screen?.capture === true} active={step === 'capture'}>
                {canRequest ? request('capture', 'Allow screen recording') : null}
                {access.requested.has('capture') ? <SettingsLink action={() => access.openSettings('capture')} /> : null}
              </AccessRow>
              <AccessRow title="Control apps" icon="cursorarrow.motionlines" detail="Lets Codync click, type and interact with apps on your behalf." done={screen?.input === true} active={step === 'input'}>
                {canRequest ? request('input', 'Allow computer control') : null}
                {access.requested.has('input') ? <SettingsLink action={() => access.openSettings('input')} /> : null}
              </AccessRow>
            </> : <AccessRow title="Screen sharing & control" icon="cursorarrow.motionlines" detail="Choose a screen and allow mouse and keyboard access. You can stop sharing at any time." done={screen?.capture === true && screen.input} active={step === 'portal'}>
              {canRequest ? request('portal', 'Choose screen and allow control') : null}
            </AccessRow>}
          </>}
        </div>
        {mac && !ready ? <div className="computer-access-help">
          <Icon name="info.circle" size={13} />
          <p>In System Settings, look for <strong>{access.helperName}</strong>. Computer control may be called <strong>Device Control and Data Access</strong> or <strong>Accessibility</strong>. Return here after granting access; if asked, allow the helper to restart.</p>
        </div> : null}
        <div className="computer-access-reassurance"><Icon name="lock.shield" size={13} /><p>Permissions stay on this computer. {onContinue ? 'You can change them later in Settings → Computer access.' : 'Your other computers keep their own permissions.'}</p></div>
        <footer className="computer-access-footer">
          <Button kind="secondary" onClick={() => void access.refresh()}><Icon name="arrow.clockwise" size={12} /> Check again</Button>
          {onContinue ? <Button kind={ready ? 'primary' : 'secondary'} onClick={() => onContinue(ready)}>{ready ? 'Continue' : 'Set up later'}{ready ? <Icon name="arrow.right" size={12} /> : null}</Button> : null}
        </footer>
        <p className="computer-access-phone-note">On iPhone, connect with a QR code or Ask Access. No SSH setup needed.</p>
      </div>
    </section>
  )
}

function AccessRow({ title, icon, detail, done, active = false, status, children }: {
  title: string; icon: string; detail: string; done: boolean; active?: boolean; status?: string; children?: ReactNode
}) {
  return <div className={`computer-access-row ${done ? 'granted' : active ? 'current' : 'waiting'}`}>
    <span className="computer-access-row-icon"><Icon name={icon} size={17} /></span>
    <div className="computer-access-row-body">
      <div className="computer-access-row-heading"><h2>{title}</h2><span className="computer-access-status">{done ? <><Icon name="checkmark" size={10} weight="semibold" /> Ready</> : status ?? (active ? 'Needs access' : 'Waiting')}</span></div>
      <p>{detail}</p>
      {!done ? <div className="computer-access-actions">{children}</div> : null}
    </div>
  </div>
}

function AccessNotice({ title, icon, error = false, children }: { title: string; icon: string; error?: boolean; children: ReactNode }) {
  return <div className={`computer-access-notice ${error ? 'error' : ''}`} role={error ? 'alert' : 'status'}>
    <Icon name={icon} size={16} /><div><strong>{title}</strong><p>{children}</p></div>
  </div>
}

function SettingsLink({ action }: { action: () => void }) {
  return <Button kind="secondary" onClick={action}>Open System Settings <Icon name="arrow.up.right.square" size={10} /></Button>
}

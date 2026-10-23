import type { ReactNode } from 'react'
import { Button, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { useTr } from '../../lib/use-language'
import { useScreenViewed } from '../../store/analytics'
import { useComputerAccess } from './use-computer-access'
import appIcon from '../../../../resources/icon.png'
import './computer-access.css'

/** Shared by onboarding and Settings. Permission requests remain explicit and local. */
export function ComputerAccessView({ onContinue }: { onContinue?: (ready: boolean) => void }) {
  const access = useComputerAccess()
  const tr = useTr()
  useScreenViewed('computer_access')
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
      {busy === permission ? <><Spinner /> {tr('Waiting for permission…')}</> : title}
    </Button>
  )
  return (
    <section className={`computer-access ${onContinue ? 'onboarding' : ''}`} aria-label={tr('Computer access setup')}>
      <div className="computer-access-content">
        <header className="computer-access-header">
          <div className="computer-access-identity">
            <img src={appIcon} width={40} height={40} alt="Codync" />
            <div><span>{tr('This computer')}</span><strong>{access.computerName}</strong></div>
          </div>
          <h1>{ready ? tr('Your computer is ready') : tr('Computer access')}</h1>
          <p>{tr(windows ? 'Let a bot work in your apps. You choose which bots can use this computer.' : 'Work in your apps with a bot, or access your screen from your phone. You choose which bots can use this computer.')}</p>
        </header>
        {step === 'host' ? <AccessNotice title="Waiting for your computer" icon="desktopcomputer">
          Start or restart the host from Codync’s menu to continue. We’ll check again automatically.
        </AccessNotice> : null}
        {step === 'unavailable' ? <AccessNotice title="Codync Screen is missing" icon="desktopcomputer.trianglebadge.exclamationmark">
          Install the full Codync desktop app to finish setup.
        </AccessNotice> : null}
        {access.error ? <AccessNotice title="Couldn’t finish setup" icon="exclamationmark.circle" error>{access.error}</AccessNotice> : null}
        <div className="computer-access-section-label">
          <span>{tr(windows ? 'Availability' : 'Permissions')}</span>
          <span>{ready ? tr('All set') : tr('{done} of {total} ready').replace('{done}', String(completed)).replace('{total}', String(mac ? 3 : windows ? 1 : 2))}<IconButton title="Check again" icon="arrow.clockwise" size={22} onClick={() => void access.refresh()} /></span>
        </div>
        <div className="computer-access-steps">
          {windows ? <AccessRow title="Computer control" icon="cursorarrow.motionlines" detail="Bots can use apps on this computer. Remote screen viewing from your phone isn’t available on Windows yet." done={ready} /> : <>
            <AccessRow title="Codync Screen" icon="desktopcomputer" detail="Runs in the background to share your screen and connect your controls." done={helperReady} active={['enable', 'helper', 'background'].includes(step)} status={step === 'background' ? 'Needs approval' : undefined}>
              {['enable', 'helper'].includes(step) ? <Button disabled={busy !== null} onClick={() => void access.enable()}>{busy === 'enable' ? <><Spinner /> {tr('Starting…')}</> : step === 'helper' ? 'Start again' : 'Enable Codync Screen'}</Button> : null}
              {step === 'background' ? <><p className="computer-access-inline-note">{tr('Allow Codync to run in the background in Login Items.')}</p><Button onClick={() => access.openSettings('background')}>{tr('Open Login Items')} <Icon name="arrow.up.right.square" size={10} /></Button></> : null}
            </AccessRow>
            {mac ? <>
              <AccessRow title="Screen recording" icon="record.circle" detail="Lets Codync see your screen, including anything visible in your apps." done={screen?.capture === true} active={step === 'capture'}>
                {step === 'capture' ? <SettingsHint app={access.helperName} /> : null}
                {canRequest ? request('capture', 'Allow screen recording') : null}
                {access.requested.has('capture') ? <SettingsLink action={() => access.openSettings('capture')} /> : null}
              </AccessRow>
              <AccessRow title="Control apps" icon="cursorarrow.motionlines" detail="Lets Codync click, type and interact with apps on your behalf." done={screen?.input === true} active={step === 'input'}>
                {step === 'input' ? <SettingsHint app={access.helperName} control /> : null}
                {canRequest ? request('input', 'Allow computer control') : null}
                {access.requested.has('input') ? <SettingsLink action={() => access.openSettings('input')} /> : null}
              </AccessRow>
            </> : <AccessRow title="Screen sharing & control" icon="cursorarrow.motionlines" detail="Choose a screen and allow mouse and keyboard access. You can stop sharing at any time." done={screen?.capture === true && screen.input} active={step === 'portal'}>
              {canRequest ? request('portal', 'Choose screen and allow control') : null}
            </AccessRow>}
          </>}
        </div>
        <ul className="computer-access-notes">
          <li><Icon name="lock.shield" size={13} /><p>{tr(onContinue ? 'Permissions stay on this computer. You can change them later in Settings → Computer access.' : 'Permissions stay on this computer. Your other computers keep their own permissions.')}</p></li>
          <li><Icon name="iphone" size={13} /><p>{tr('On iPhone, connect with a QR code or Ask Access. No SSH setup needed.')}</p></li>
        </ul>
        {onContinue ? <footer className="computer-access-footer">
          <Button kind={ready ? 'primary' : 'secondary'} onClick={() => onContinue(ready)}>{tr(ready ? 'Continue' : 'Set up later')}{ready ? <Icon name="arrow.right" size={12} /> : null}</Button>
        </footer> : null}
      </div>
    </section>
  )
}

function AccessRow({ title, icon, detail, done, active = false, status, children }: {
  title: string; icon: string; detail: string; done: boolean; active?: boolean; status?: string; children?: ReactNode
}) {
  const tr = useTr()
  const label = tr(done ? 'Ready' : status ?? (active ? 'Needs access' : 'Waiting'))
  return <div className={`computer-access-row ${done ? 'granted' : active ? 'current' : 'waiting'}`}>
    <span className="computer-access-badge" role="img" aria-label={label} title={label}>
      {done ? <Icon name="checkmark" size={12} weight="bold" /> : <Icon name={icon} size={15} />}
    </span>
    <div className="computer-access-row-body">
      <h2>{tr(title)}</h2>
      <p>{tr(detail)}</p>
      {!done ? <div className="computer-access-actions">{children}</div> : null}
    </div>
  </div>
}

function AccessNotice({ title, icon, error = false, children }: { title: string; icon: string; error?: boolean; children: ReactNode }) {
  const tr = useTr()
  return <div className={`computer-access-notice ${error ? 'error' : ''}`} role={error ? 'alert' : 'status'}>
    <Icon name={icon} size={16} /><div><strong>{tr(title)}</strong><p>{typeof children === 'string' ? tr(children) : children}</p></div>
  </div>
}

function SettingsHint({ app, control = false }: { app: string; control?: boolean }) {
  const tr = useTr()
  return <p className="computer-access-inline-note">
    {tr('In System Settings, look for')} <strong>{app}</strong>.{control ? <> {tr('It may be listed under Device Control and Data Access or Accessibility.')}</> : null} {tr('Return here after granting access; if asked, allow the helper to restart.')}
  </p>
}

function SettingsLink({ action }: { action: () => void }) {
  const tr = useTr()
  return <Button kind="secondary" onClick={action}>{tr('Open System Settings')} <Icon name="arrow.up.right.square" size={10} /></Button>
}

import type { ReactNode } from 'react'
import { Button, IconButton } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { useTr } from '../../lib/use-language'
import { useScreenViewed } from '../../store/analytics'
import { useComputerAccess } from './use-computer-access'
import { AccessSteps } from './AccessSteps'
import appIcon from '../../../../resources/icon.png'
import './computer-access.css'

/** Shared by onboarding and Settings. Permission requests remain explicit and local. */
export function ComputerAccessView({ onContinue }: { onContinue?: (ready: boolean) => void }) {
  const access = useComputerAccess()
  const tr = useTr()
  useScreenViewed('computer_access')
  const { screen, step } = access
  const mac = window.codync.platform === 'darwin'
  const windows = window.codync.platform === 'win32'
  const ready = step === 'ready'
  const helperReady = screen?.enabled === true && screen.connected && step !== 'background'
  const completed = windows ? Number(ready) : Number(helperReady) + (mac
    ? Number(screen?.capture === true) + Number(screen?.input === true)
    : Number(screen?.capture === true && screen.input))
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
        <div className="computer-access-steps"><AccessSteps access={access} /></div>
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

export function AccessNotice({ title, icon, error = false, children }: { title: string; icon: string; error?: boolean; children: ReactNode }) {
  const tr = useTr()
  return <div className={`computer-access-notice ${error ? 'error' : ''}`} role={error ? 'alert' : 'status'}>
    <Icon name={icon} size={16} /><div><strong>{tr(title)}</strong><p>{typeof children === 'string' ? tr(children) : children}</p></div>
  </div>
}

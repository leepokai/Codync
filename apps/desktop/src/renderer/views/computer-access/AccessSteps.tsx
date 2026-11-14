import type { ReactNode } from 'react'
import type { ComputerAccess } from '../../store/computer-access'
import type { AccessPage } from '../../lib/access-onboarding'
import { Button, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { useTr } from '../../lib/use-language'

export function AccessSteps({ access, selection }: { access: ComputerAccess; selection?: AccessPage }) {
  const tr = useTr()
  const { screen, step, busy } = access
  const mac = window.codync.platform === 'darwin'
  const windows = window.codync.platform === 'win32'
  const ready = step === 'ready'
  const canRequest = ['capture', 'input', 'portal', 'ready'].includes(step)
  const helperReady = screen?.enabled === true && screen.connected && step !== 'background'
  const request = (permission: 'capture' | 'input' | 'portal', title: string) => (
    <Button kind={step === permission ? 'primary' : 'secondary'} disabled={!canRequest || busy !== null} onClick={() => void access.request(permission)}>
      {busy === permission ? <><Spinner /> {tr('Waiting for permission…')}</> : title}
    </Button>
  )
  return (
    <>
      {windows ? <AccessRow large={selection !== undefined} title="Computer control" icon="cursorarrow.motionlines" detail="Bots can use apps on this computer. Remote screen viewing from your phone isn’t available on Windows yet." done={ready} /> : <>
        {(!selection || selection === 'helper') ? <AccessRow large={selection !== undefined} title="Codync Screen" icon="desktopcomputer" detail="Runs in the background to share your screen and connect your controls." done={helperReady} active={['enable', 'helper', 'background'].includes(step)} status={step === 'background' ? 'Needs approval' : undefined}>
          {['enable', 'helper'].includes(step) ? <Button disabled={busy !== null} onClick={() => void access.enable()}>{busy === 'enable' ? <><Spinner /> {tr('Starting…')}</> : step === 'helper' ? 'Start again' : 'Enable Codync Screen'}</Button> : null}
          {step === 'background' ? <><p className="computer-access-inline-note">{tr('Allow Codync to run in the background in Login Items.')}</p><Button onClick={() => access.openSettings('background')}>{tr('Open Login Items')} <Icon name="arrow.up.right.square" size={10} /></Button></> : null}
        </AccessRow> : null}
        {mac ? <>
          {(!selection || selection === 'capture') ? <AccessRow large={selection !== undefined} title="Screen recording" icon="record.circle" detail="Lets Codync see your screen, including anything visible in your apps." done={screen?.capture === true} active={step === 'capture'}>
            {step === 'capture' ? <SettingsHint app={access.helperName} /> : null}
            {canRequest ? request('capture', 'Allow screen recording') : null}
            {access.requested.has('capture') ? <SettingsLink action={() => access.openSettings('capture')} /> : null}
          </AccessRow> : null}
          {(!selection || selection === 'input') ? <AccessRow large={selection !== undefined} title="Control apps" icon="cursorarrow.motionlines" detail="Lets Codync click, type and interact with apps on your behalf." done={screen?.input === true} active={step === 'input'}>
            {step === 'input' ? <SettingsHint app={access.helperName} control /> : null}
            {canRequest ? request('input', 'Allow computer control') : null}
            {access.requested.has('input') ? <SettingsLink action={() => access.openSettings('input')} /> : null}
          </AccessRow> : null}
        </> : (!selection || selection === 'portal') ? <AccessRow large={selection !== undefined} title="Screen sharing & control" icon="cursorarrow.motionlines" detail="Choose a screen and allow mouse and keyboard access. You can stop sharing at any time." done={screen?.capture === true && screen.input} active={step === 'portal'}>
          {canRequest ? request('portal', 'Choose screen and allow control') : null}
        </AccessRow> : null}
      </>}
    </>
  )
}

function AccessRow({ title, icon, detail, done, active = false, status, children, large = false }: {
  title: string; icon: string; detail: string; done: boolean; active?: boolean; status?: string; children?: ReactNode; large?: boolean
}) {
  const tr = useTr()
  const label = tr(done ? 'Ready' : status ?? (active ? 'Needs access' : 'Waiting'))
  return <div className={`computer-access-row ${done ? 'granted' : active ? 'current' : 'waiting'}`}>
    <span className="computer-access-badge" role="img" aria-label={label} title={label}>
      {done ? <Icon name="checkmark" size={large ? 26 : 12} weight="bold" /> : <Icon name={icon} size={large ? 26 : 15} />}
    </span>
    <div className="computer-access-row-body">
      <h2>{tr(title)}</h2>
      <p>{tr(detail)}</p>
      {!done ? <div className="computer-access-actions">{children}</div> : null}
    </div>
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

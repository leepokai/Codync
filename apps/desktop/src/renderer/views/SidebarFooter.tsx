import type { AutomanState } from '@shared/models'
import { useTr } from '../lib/use-language'
import { Icon } from '../components/Icon'
import { useApp } from '../store/context'
import { ProfileAvatar } from './PanelMenus'
import { UpdateBadge } from './UpdateButton'
import gmail from '../assets/gmail.png'
import calendar from '../assets/google-calendar.png'
import drive from '../assets/google-drive.png'
import './sidebar-footer.css'

/** Account and app connections float above the scrolling roster; Extensions only when given. */
export function SidebarFooter({ accountOpen, onAccount, onExtensions, onAutomations }: {
  accountOpen: boolean
  onAccount: () => void
  onExtensions?: () => void
  onAutomations: () => void
}) {
  const app = useApp()
  const automan = app.local?.automan ?? null
  const tr = useTr()
  return (
    <div className="sidebar-footer">
      <div className="sidebar-footer-fade" aria-hidden />
      {onExtensions ? (
        <>
          <button
            className="automations"
            title={tr('Automations: the n8n editor, run by this computer')}
            onClick={onAutomations}
          >
            <Icon name="point.3.connected.trianglepath.dotted" size={13} />
            <span>{tr('Automations')}</span>
            <span className={`automan-dot ${dotState(automan)}`} aria-hidden />
          </button>
          <div className="sidebar-footer-row">
            <button
              className={`sidebar-account ${accountOpen ? 'active' : ''}`}
              aria-label="Open account menu for Account"
              aria-expanded={accountOpen}
              title="Account menu"
              onClick={onAccount}
            >
              <ProfileAvatar />
            </button>
            <button className="extensions" onClick={onExtensions}>
              <span>{tr('Extensions')}</span>
              <span className="extensions-logos" aria-hidden>
                {[gmail, calendar, drive].map((src) => <img key={src} src={src} alt="" draggable={false} />)}
              </span>
            </button>
            <UpdateBadge size={34} />
          </div>
        </>
      ) : null}
    </div>
  )
}

function dotState(automan: AutomanState | null): string {
  if (automan?.state === 'running') return 'running'
  if (automan?.state === 'starting') return 'starting'
  if (automan?.state === 'error') return 'error'
  return 'off'
}

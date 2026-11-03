import type { N8nState } from '@shared/models'
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
export function SidebarFooter({ accountOpen, onAccount, onExtensions, onN8n }: {
  accountOpen: boolean
  onAccount: () => void
  onExtensions?: () => void
  onN8n: () => void
}) {
  const app = useApp()
  const n8n = app.local?.n8n ?? null
  const tr = useTr()
  return (
    <div className="sidebar-footer">
      <div className="sidebar-footer-fade" aria-hidden />
      {onExtensions ? (
        <>
          <button
            className="n8n"
            title={tr('n8n: the workflow editor, run by this computer')}
            onClick={onN8n}
          >
            <Icon name="point.3.connected.trianglepath.dotted" size={13} />
            <span>{tr('n8n')}</span>
            <span className={`n8n-dot ${dotState(n8n)}`} aria-hidden />
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

function dotState(n8n: N8nState | null): string {
  if (n8n?.state === 'running') return 'running'
  if (n8n?.state === 'starting') return 'starting'
  if (n8n?.state === 'error') return 'error'
  return 'off'
}

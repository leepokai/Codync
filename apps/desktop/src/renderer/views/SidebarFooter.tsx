import { useTr } from '../lib/use-language'
import { ProfileAvatar } from './PanelMenus'
import { UpdateBadge } from './UpdateButton'
import gmail from '../assets/gmail.png'
import calendar from '../assets/google-calendar.png'
import drive from '../assets/google-drive.png'
import './sidebar-footer.css'

/** Account and app connections float above the scrolling roster; Extensions only when given. */
export function SidebarFooter({ accountOpen, onAccount, onExtensions }: {
  accountOpen: boolean
  onAccount: () => void
  onExtensions?: () => void
}) {
  const tr = useTr()
  return (
    <div className="sidebar-footer">
      <div className="sidebar-footer-fade" aria-hidden />
      <button
        className={`sidebar-account ${accountOpen ? 'active' : ''}`}
        aria-label="Open account menu for Account"
        aria-expanded={accountOpen}
        title="Account menu"
        onClick={onAccount}
      >
        <ProfileAvatar />
      </button>
      {onExtensions ? (
        <button className="extensions" onClick={onExtensions}>
          <span>{tr('Extensions')}</span>
          <span className="extensions-logos" aria-hidden>
            {[gmail, calendar, drive].map((src) => <img key={src} src={src} alt="" draggable={false} />)}
          </span>
        </button>
      ) : null}
      <UpdateBadge size={34} />
    </div>
  )
}

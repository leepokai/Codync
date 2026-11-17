import type { TraySymbol } from '@shared/ipc'
import type { Computer } from '@shared/models'
import { Icon } from '../components/Icon'
import { avatarColor } from '../lib/theme'
import linux from '../assets/computer-linux.svg'
import './computer-badge.css'

const SYMBOLS: Record<string, TraySymbol> = { laptop: 'laptopcomputer', macmini: 'macmini', macstudio: 'macstudio', imac: 'desktopcomputer', macpro: 'macpro.gen3', cloud: 'cloud' }

/** The SF Symbol for what a computer is, which the tray menu shows too. */
export const computerSymbol = (device?: string | null): TraySymbol => SYMBOLS[device ?? ''] ?? 'desktopcomputer'

/** A computer's icon showing what it is (laptop, Mac mini, Linux…), in the color picked for it. */
export function ComputerBadge({ computer, size = 36 }: { computer: Computer | null; size?: number }) {
  const color = computer?.color ? avatarColor(computer.color) : 'var(--text)'
  return (
    <span aria-hidden className="computer-badge" style={{ width: size, height: size, color }}>
      {computer?.device === 'linux' ? (
        // SF Symbols has no penguin.
        <span className="computer-badge-linux" style={{ width: size * 0.55, height: size * 0.55, backgroundColor: color, WebkitMaskImage: `url("${linux}")` }} />
      ) : (
        <Icon name={computerSymbol(computer?.device)} size={size * 0.5} weight="medium" />
      )}
    </span>
  )
}

import type { ReactNode } from 'react'
import { avatarColor } from '../lib/theme'

export const BOT_AVATAR_LOOKS = [
  'round',
  'visor',
  'dome',
  'boxy',
  'cyclops',
  'ears',
  'scout',
  'owl',
  'pixel',
  'hex',
  'plush',

  'astro',
  'cat',
] as const

export type BotAvatarLook = (typeof BOT_AVATAR_LOOKS)[number]

export function isRobotLook(look?: string | null): look is BotAvatarLook {
  return !!look && (BOT_AVATAR_LOOKS as readonly string[]).includes(look)
}

function renderHead(look: BotAvatarLook): ReactNode {
  switch (look) {
    case 'round':
      return (
        <>
          <line x1="12" y1="3" x2="12" y2="6" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
          <circle cx="12" cy="2.5" r="1.4" fill="currentColor" />
          <rect x="4.5" y="6" width="15" height="13" rx="6.5" stroke="currentColor" strokeWidth="1.8" />
          <circle cx="9.5" cy="12" r="1.5" fill="currentColor" />
          <circle cx="14.5" cy="12" r="1.5" fill="currentColor" />
          <path d="M9.5 15.5c1.5 1 3.5 1 5 0" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
        </>
      )
    case 'visor':
      return (
        <>
          <rect x="4" y="5.5" width="16" height="13" rx="4" stroke="currentColor" strokeWidth="1.8" />
          <rect x="6.5" y="9" width="11" height="4" rx="2" fill="currentColor" />
          <line x1="9" y1="15.5" x2="15" y2="15.5" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
        </>
      )
    case 'dome':
      return (
        <>
          <path d="M5 13a7 7 0 0 1 14 0v4a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2v-4Z" stroke="currentColor" strokeWidth="1.8" />
          <circle cx="9.5" cy="12.5" r="1.4" fill="currentColor" />
          <circle cx="14.5" cy="12.5" r="1.4" fill="currentColor" />
          <line x1="12" y1="3" x2="12" y2="6" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
        </>
      )
    case 'boxy':
      return (
        <>
          <rect x="4.5" y="5.5" width="15" height="13" rx="2" stroke="currentColor" strokeWidth="1.8" />
          <rect x="7.5" y="9.5" width="2.8" height="2.8" rx="0.6" fill="currentColor" />
          <rect x="13.7" y="9.5" width="2.8" height="2.8" rx="0.6" fill="currentColor" />
          <line x1="8.5" y1="15.5" x2="15.5" y2="15.5" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
        </>
      )
    case 'cyclops':
      return (
        <>
          <rect x="4.5" y="5.5" width="15" height="13" rx="5" stroke="currentColor" strokeWidth="1.8" />
          <circle cx="12" cy="11.2" r="2.8" stroke="currentColor" strokeWidth="1.6" />
          <circle cx="12" cy="11.2" r="1.1" fill="currentColor" />
          <line x1="9.5" y1="16" x2="14.5" y2="16" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
        </>
      )
    case 'ears':
      return (
        <>
          <line x1="2.5" y1="10" x2="2.5" y2="14" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
          <line x1="21.5" y1="10" x2="21.5" y2="14" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
          <rect x="4.5" y="5.5" width="15" height="13" rx="4" stroke="currentColor" strokeWidth="1.8" />
          <circle cx="9.5" cy="11.5" r="1.5" fill="currentColor" />
          <circle cx="14.5" cy="11.5" r="1.5" fill="currentColor" />
          <path d="M9.5 15.2h5" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
        </>
      )
    case 'scout':
      return (
        <>
          <path d="M8 3.5h8l3 4.5v9a2.5 2.5 0 0 1-2.5 2.5h-9A2.5 2.5 0 0 1 5 17V8l3-4.5Z" stroke="currentColor" strokeWidth="1.8" />
          <circle cx="9.5" cy="11.5" r="1.4" fill="currentColor" />
          <circle cx="14.5" cy="11.5" r="1.4" fill="currentColor" />
          <line x1="10" y1="15.5" x2="14" y2="15.5" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
        </>
      )
    case 'owl':
      return (
        <>
          <rect x="4.5" y="5.5" width="15" height="13" rx="5" stroke="currentColor" strokeWidth="1.8" />
          <circle cx="9.2" cy="11.5" r="2.4" stroke="currentColor" strokeWidth="1.5" />
          <circle cx="14.8" cy="11.5" r="2.4" stroke="currentColor" strokeWidth="1.5" />
          <circle cx="9.2" cy="11.5" r="0.9" fill="currentColor" />
          <circle cx="14.8" cy="11.5" r="0.9" fill="currentColor" />
          <path d="M11.2 14.5 12 16l.8-1.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
        </>
      )
    case 'pixel':
      return (
        <>
          <rect x="5" y="5" width="14" height="14" stroke="currentColor" strokeWidth="1.8" />
          <rect x="8" y="9" width="2.5" height="2.5" fill="currentColor" />
          <rect x="13.5" y="9" width="2.5" height="2.5" fill="currentColor" />
          <path d="M8.5 15h7" stroke="currentColor" strokeWidth="1.8" />
        </>
      )
    case 'hex':
      return (
        <>
          <polygon points="12,3.5 19.5,7.8 19.5,16.2 12,20.5 4.5,16.2 4.5,7.8" stroke="currentColor" strokeWidth="1.8" fill="none" />
          <circle cx="9.5" cy="11.8" r="1.4" fill="currentColor" />
          <circle cx="14.5" cy="11.8" r="1.4" fill="currentColor" />
          <line x1="9.8" y1="15.5" x2="14.2" y2="15.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
        </>
      )
    case 'astro':
      return (
        <>
          <circle cx="12" cy="12" r="8" stroke="currentColor" strokeWidth="1.8" />
          <ellipse cx="12" cy="11.5" rx="5.2" ry="3.4" stroke="currentColor" strokeWidth="1.5" />
          <circle cx="10.2" cy="11.5" r="1" fill="currentColor" />
          <circle cx="13.8" cy="11.5" r="1" fill="currentColor" />
        </>
      )
    case 'cat':
      return (
        <>
          <path d="M5 17V6l3.5 2.8h7L19 6v11a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2Z" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" />
          <circle cx="9.5" cy="12.5" r="1.4" fill="currentColor" />
          <circle cx="14.5" cy="12.5" r="1.4" fill="currentColor" />
          <path d="M10.5 15.6c.9.7 2.1.7 3 0" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
        </>
      )
    case 'plush':
      return (
        <>
          {/* A felt face: the fur is a ring of short strokes, the eyes large and open. */}
          <circle cx="12" cy="12" r="8.4" fill="currentColor" />
          <circle cx="12" cy="12" r="9.6" fill="none" stroke="currentColor" strokeWidth="1.1" strokeDasharray="0.7 1.2" strokeLinecap="round" />
          <circle cx="9" cy="11.2" r="2.1" fill="var(--background)" />
          <circle cx="15" cy="11.2" r="2.1" fill="var(--background)" />
          <circle cx="9" cy="11.4" r="0.95" fill="currentColor" />
          <circle cx="15" cy="11.4" r="0.95" fill="currentColor" />
          <path d="M10.2 15c1 .9 2.6.9 3.6 0" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" fill="none" />
        </>
      )
  }
}

/** Ideas 21 & 22: Geometric robot avatar with 12 customizable looks and working spinner crossfade. */
export function RobotAvatar({
  look = 'round',
  color = 'blue',
  size = 40,
  working = false,
}: {
  look?: BotAvatarLook
  color?: string
  size?: number
  working?: boolean
}) {
  const accent = avatarColor(color)
  const iconSize = Math.round(size * 0.62)
  return (
    <div
      className={`bot-avatar-wrap ${working ? 'is-working' : ''}`}
      style={{
        width: size,
        height: size,
        borderRadius: Math.round(size * 0.3),
        background: `color-mix(in srgb, ${accent} 18%, var(--surface))`,
        color: accent,
      }}
    >
      <svg
        className="bot-avatar-face"
        width={iconSize}
        height={iconSize}
        viewBox="0 0 24 24"
        fill="none"
        aria-hidden
      >
        {renderHead(look)}
      </svg>
      <span className="bot-avatar-spinner" aria-hidden>
        <svg
          className="bot-avatar-spin-svg"
          width={iconSize}
          height={iconSize}
          viewBox="0 0 24 24"
          fill="none"
        >
          <circle
            cx="12"
            cy="12"
            r="8.5"
            stroke="currentColor"
            strokeOpacity="0.22"
            strokeWidth="2.2"
          />
          <path
            d="M12 3.5a8.5 8.5 0 0 1 8.5 8.5"
            stroke="currentColor"
            strokeWidth="2.2"
            strokeLinecap="round"
          />
        </svg>
      </span>
    </div>
  )
}

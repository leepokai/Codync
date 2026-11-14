import { useState, type ReactNode } from 'react'
import { validIcon, type CustomIcon as IconData } from '@shared/custom-icons'

/** Static normalized image, including when the bot is working. */
export function CustomIcon({ icon, size, color = 'currentColor', fallback }: {
  icon?: IconData; size: number; color?: string; fallback: ReactNode
}) {
  const [failed, setFailed] = useState<string | null>(null)
  if (!validIcon(icon) || failed === icon.dataUrl) return fallback
  return <img src={icon.dataUrl} alt="" width={size} height={size} onError={() => setFailed(icon.dataUrl)}
    style={{ objectFit: 'contain', flex: 'none', ...(icon.tinted ? {
      background: color, objectPosition: '-9999px', maskImage: `url("${icon.dataUrl}")`,
      maskSize: 'contain', maskPosition: 'center', maskRepeat: 'no-repeat',
    } : {}) }} />
}

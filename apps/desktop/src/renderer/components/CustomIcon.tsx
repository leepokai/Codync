import { useContext, useState, type ReactNode } from 'react'
import { validIcon, type CustomIcon as IconData } from '@shared/custom-icons'
import { StoreContext } from '../store/context'
import { useModel } from '../lib/observable'

export function useAgentIcon(registry?: string | null) {
  const store = useModel(useContext(StoreContext))
  const backend = store?.hello?.backends.find((b) => b.id === registry || b.registry === registry)
  return store?.icons[`agent:${backend?.id ?? registry}`]
}

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

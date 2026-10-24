import type { UsageProvider } from '@shared/models'
import { agentIconURL, AgentIcon } from '../../components/AgentIcon'
import { CharacterAvatar, drawCharacter } from '../../components/Avatar'
import { Icon } from '../../components/Icon'
import { HStack } from '../../components/Stack'
import { palette } from '../../lib/theme'
import { mascotShape, providerRegistry, providerTint, providerTintCSS, tightest } from './usage-model'
import './usage.css'

/** Whether any of the provider's limits is nearly used up (its mascot shows a red alert). */
export function nearlyUsedUp(provider: UsageProvider) {
  return (tightest(provider)?.percent ?? 0) >= 90
}

/** A provider's character, with a red "!" once any of its limits is nearly used up. */
export function ProviderMascot({ provider, size, style = 'character' }: { provider: UsageProvider; size: number; style?: 'character' | 'original' }) {
  const full = nearlyUsedUp(provider)
  if (style === 'original') {
    return (
      <span className="usage-mascot-logo" style={{ width: size, height: size }}>
        <AgentIcon registry={providerRegistry(provider.id)} size={size * 0.84} color={providerTintCSS(provider.id)} />
      </span>
    )
  }
  return (
    <span className="usage-mascot" style={{ width: size, height: size }}>
      <CharacterAvatar shape={mascotShape(provider.id)} color="" tint={providerTint(provider.id)} size={size} mood={full ? 'needsInput' : 'idle'} />
      {full ? (
        <HStack as="span" align="stretch" className="usage-mascot-alert" style={{ top: -size * 0.12, right: -size * 0.12 }}>
          <Icon name="exclamationmark.circle.fill" size={size * 0.34} weight="bold" scaled={false} />
        </HStack>
      ) : null}
    </span>
  )
}

const loaded = new Map<string, HTMLImageElement>()

/** Draws the mascot at the context's origin; false while an original logo loads (it appears on the next update). */
export function drawMascot(ctx: CanvasRenderingContext2D, provider: UsageProvider, size: number, style: 'character' | 'original', dark: boolean): boolean {
  const tint = provider.id === 'claude' ? '#D97757' : dark ? '#FFFFFF' : '#000000'
  if (style === 'original') {
    const url = agentIconURL(providerRegistry(provider.id))
    if (!url) return false
    const image = loaded.get(url)
    if (!image) {
      const img = new Image()
      img.src = url
      img.onload = () => loaded.set(url, img)
      return false
    }
    const inset = size * 0.08
    ctx.drawImage(image, inset, inset, size - inset * 2, size - inset * 2)
    ctx.globalCompositeOperation = 'source-in'
    ctx.fillStyle = tint
    ctx.fillRect(0, 0, size, size)
    return true
  }
  const full = nearlyUsedUp(provider)
  drawCharacter(ctx, { shape: mascotShape(provider.id), fill: tint, size, mood: full ? 'needsInput' : 'idle', dark, t: 0, still: true })
  if (full) {
    ctx.beginPath()
    ctx.arc(size * 0.92, size * 0.08, size * 0.17, 0, Math.PI * 2)
    ctx.fillStyle = palette(dark).danger
    ctx.fill()
  }
  return true
}

/** The mascot as an image for a screen of `ratio` (the island), or null while it can't be drawn. */
export function mascotDataURL(provider: UsageProvider, size: number, style: 'character' | 'original', dark: boolean, ratio: number): string | null {
  const pixels = Math.max(1, Math.round(size * ratio))
  const canvas = document.createElement('canvas')
  canvas.width = pixels
  canvas.height = pixels
  const ctx = canvas.getContext('2d')!
  ctx.scale(pixels / size, pixels / size)
  return drawMascot(ctx, provider, size, style, dark) ? canvas.toDataURL('image/png') : null
}

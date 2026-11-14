import { validIcon, type CustomIcon } from '../../shared/custom-icons'

const bitmaps = new Map<string, { image: HTMLImageElement; ready: Promise<void> }>()

/** One decode per cached PNG. Invalid images use the caller's built-in fallback. */
export function iconBitmap(icon: CustomIcon | undefined) {
  if (!validIcon(icon)) return undefined
  let entry = bitmaps.get(icon.dataUrl)
  if (!entry) {
    const image = new Image()
    const ready = new Promise<void>((resolve) => { image.onload = () => resolve(); image.onerror = () => resolve() })
    image.src = icon.dataUrl
    entry = { image, ready }
    bitmaps.set(icon.dataUrl, entry)
    if (bitmaps.size > 128) bitmaps.delete(bitmaps.keys().next().value!)
  }
  return entry
}

export function drawIconBitmap(ctx: CanvasRenderingContext2D, icon: CustomIcon | undefined, size: number, color: string): boolean {
  const image = iconBitmap(icon)?.image
  if (!image?.complete || !image.naturalWidth) return false
  const scale = size / Math.max(image.naturalWidth, image.naturalHeight)
  const width = image.naturalWidth * scale
  const height = image.naturalHeight * scale
  ctx.save()
  ctx.drawImage(image, (size - width) / 2, (size - height) / 2, width, height)
  if (icon?.tinted) {
    ctx.globalCompositeOperation = 'source-in'
    ctx.fillStyle = color
    ctx.fillRect(0, 0, size, size)
  }
  ctx.restore()
  return true
}

/** Puts a picture on the clipboard as a picture, so it pastes into other apps as an image and not as a name. */

/** Which step of a copy failed: turning the picture into PNG, or writing the clipboard. */
export class ImageCopyError extends Error {
  readonly kind: 'convert' | 'clipboard'
  constructor(kind: 'convert' | 'clipboard', cause: unknown) {
    super(kind === 'convert' ? 'The picture could not be converted' : 'The clipboard did not take the picture', { cause })
    this.kind = kind
  }
}

/** Whether the bytes are a PNG by their own signature: a name saying `.png` doesn't make them one. */
export function isPng(bytes: Uint8Array) {
  return [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a].every((byte, i) => bytes[i] === byte)
}

/** The most pixels a conversion draws: a bigger picture is scaled down to fit, so a huge one can't exhaust the renderer's memory. */
const MAX_PIXELS = 36_000_000

/** `width` x `height` scaled down, keeping its shape, to at most `max` pixels; as it is when it already fits. */
export function fitPixels(width: number, height: number, max = MAX_PIXELS) {
  const area = width * height
  if (area <= max) return { width, height }
  const scale = Math.sqrt(max / area)
  return { width: Math.max(1, Math.floor(width * scale)), height: Math.max(1, Math.floor(height * scale)) }
}

/** How to decode a `width` x `height` picture: straight to its capped size, so a huge one is never decoded whole. */
export function decodeOptions(width: number, height: number): ImageBitmapOptions {
  const size = fitPixels(width, height)
  return { resizeWidth: size.width, resizeHeight: size.height, resizeQuality: 'high' }
}

/** A picture's size from its header: an `<img>` that loads but is never drawn doesn't decode its pixels. */
async function headerSize(blob: Blob) {
  const url = URL.createObjectURL(blob)
  try {
    const image = new Image()
    image.src = url
    await new Promise((done, fail) => { image.onload = done; image.onerror = fail })
    return { width: image.naturalWidth, height: image.naturalHeight }
  } finally {
    URL.revokeObjectURL(url)
  }
}

/** The clipboard takes PNG for pictures: other formats are decoded at their capped size and re-encoded. */
async function asPng(bytes: Uint8Array): Promise<Blob> {
  // Only a real PNG goes through as it is; anything else is decoded by what it holds, not by what it's called.
  if (isPng(bytes)) return new Blob([bytes as BlobPart], { type: 'image/png' })
  const blob = new Blob([bytes as BlobPart])
  const { width, height } = await headerSize(blob)
  const bitmap = await createImageBitmap(blob, decodeOptions(width, height))
  try {
    const canvas = document.createElement('canvas')
    canvas.width = bitmap.width
    canvas.height = bitmap.height
    canvas.getContext('2d')?.drawImage(bitmap, 0, 0)
    return await new Promise<Blob>((done, fail) => canvas.toBlob((png) => (png ? done(png) : fail(new Error('The picture could not be converted'))), 'image/png'))
  } finally {
    bitmap.close()
  }
}

/** Numbers each copy as it starts, so one that finishes after a newer one began knows it was overtaken. */
export class CopyRequests {
  private latest = 0
  start() { return ++this.latest }
  isLatest(request: number) { return request === this.latest }
}

/** Every copy of a picture or its name: whichever started last owns the clipboard. */
export const copies = new CopyRequests()

/** The picture write the clipboard is busy with, settled either way: text waits for it, so a picture can't land over newer text. */
let writing: Promise<unknown> = Promise.resolve()

/** Text (a picture's name) on the clipboard, overtaking a picture copy still converting or being written so it can't land over it. */
export async function copyText(text: string, write: (text: string) => void) {
  const request = copies.start()
  await writing
  if (copies.isLatest(request)) write(text)
}

/** The picture on the clipboard, unless `stillWanted` says a newer copy took its place while it was converting. */
export async function copyImage(bytes: Uint8Array, stillWanted: () => boolean) {
  const png = await asPng(bytes).catch((error) => { throw new ImageCopyError('convert', error) })
  if (!stillWanted()) return
  const write = navigator.clipboard.write([new ClipboardItem({ 'image/png': png })])
  writing = write.catch(() => undefined)
  await write.catch((error) => { throw new ImageCopyError('clipboard', error) })
}

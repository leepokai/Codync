/** Puts a picture on the clipboard as a picture, so it pastes into other apps as an image and not as a name. */

const TYPES: Record<string, string> = { png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp', bmp: 'image/bmp' }

/** The MIME type a picture's file name says it is; unknown ones are left for the page to sniff. */
export function imageType(name: string) {
  return TYPES[name.split('.').pop()?.toLowerCase() ?? ''] ?? ''
}

/** The clipboard takes PNG for pictures: other formats are drawn and re-encoded. */
/** Which step of a copy failed: turning the picture into PNG, or writing the clipboard. */
export class ImageCopyError extends Error {
  readonly kind: 'convert' | 'clipboard'
  constructor(kind: 'convert' | 'clipboard', cause: unknown) {
    super(kind === 'convert' ? 'The picture could not be converted' : 'The clipboard did not take the picture', { cause })
    this.kind = kind
  }
}

async function asPng(bytes: Uint8Array, name: string): Promise<Blob> {
  const type = imageType(name)
  const blob = new Blob([bytes as BlobPart], { type })
  if (type === 'image/png') return blob
  const bitmap = await createImageBitmap(blob)
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

export async function copyImage(bytes: Uint8Array, name: string) {
  const png = await asPng(bytes, name).catch((error) => { throw new ImageCopyError('convert', error) })
  await navigator.clipboard.write([new ClipboardItem({ 'image/png': png })]).catch((error) => { throw new ImageCopyError('clipboard', error) })
}

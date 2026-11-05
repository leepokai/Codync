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

/** The clipboard takes PNG for pictures: other formats are drawn and re-encoded. */
async function asPng(bytes: Uint8Array): Promise<Blob> {
  // Only a real PNG goes through as it is; anything else is decoded by what it holds, not by what it's called.
  if (isPng(bytes)) return new Blob([bytes as BlobPart], { type: 'image/png' })
  const blob = new Blob([bytes as BlobPart])
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

export async function copyImage(bytes: Uint8Array) {
  const png = await asPng(bytes).catch((error) => { throw new ImageCopyError('convert', error) })
  await navigator.clipboard.write([new ClipboardItem({ 'image/png': png })]).catch((error) => { throw new ImageCopyError('clipboard', error) })
}

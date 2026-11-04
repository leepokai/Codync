import { isImageName, showsAsImage, type Entry } from '../../../shared/models.ts'
import type { ImageSource } from './Attachments'

/** The first picture a message holds: one a bot shared, or one sent with it. */
function imageOf(entry: Entry): ImageSource | null {
  const shared = entry.data.files?.find(showsAsImage)
  if (shared) return { file: shared, entryId: entry.id }
  const sent = entry.data.attachments?.find((file) => isImageName(file.name))
  return sent ? { file: sent, botId: entry.botId } : null
}

/**
 * The picture a message's Copy copies instead of its text: the picture of a bot's shared file (even with a caption), or a sent
 * picture with no text. A message with text and a picture keeps copying the text; one with no picture has nothing else to copy.
 */
export function copiedPicture(entry: Entry): ImageSource | null {
  const picture = imageOf(entry)
  return picture && (entry.data.files?.length || !entry.data.text) ? picture : null
}

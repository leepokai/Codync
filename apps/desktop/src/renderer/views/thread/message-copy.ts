import { isImageName, showsAsImage, type Entry } from '../../../shared/models.ts'
import type { ImageSource } from './Attachments'
import type { MenuItem } from '../../components/Menu'
import type { MessageMenu } from './use-image-actions'

/**
 * The picture a message's Copy copies instead of its text: a bot's shared file when it is the message's only file (even with a
 * caption), or a sent picture when it is the only attachment and there is no text. Anything else (text with a picture, several
 * files) keeps copying the text: each picture's own menu copies that picture.
 */
export function copiedPicture(entry: Entry): ImageSource | null {
  const { files, attachments, text } = entry.data
  if (files?.length === 1) return showsAsImage(files[0]) ? { file: files[0], entryId: entry.id } : null
  if (attachments?.length === 1 && !text) return isImageName(attachments[0].name) ? { file: attachments[0], botId: entry.botId } : null
  return null
}

/** A message's menu inside the open viewer: what its items do leaves the viewer first (Reply, what it did); reactions stay. */
export function leavingViewer(menu: MessageMenu, close: () => void): MessageMenu {
  const leave = (item: MenuItem): MenuItem => {
    if (item.submenu) return { ...item, submenu: item.submenu.map(leave) }
    return { ...item, action: () => { close(); item.action() } }
  }
  return { ...menu, items: () => menu.items().map(leave) }
}

import type { ReactNode } from 'react'
import { AnchoredMenu, useContextMenu } from '../../components/Menu'
import { useStoreRef } from '../../store/context'
import type { ImageSource } from './Attachments'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { imageCopyItems, useCopyFailed, type MessageMenu } from './use-image-actions'

/**
 * Right-click on a picture: its message's own menu (reactions, Reply, …) with the message's Copy turned into Copy image
 * and Copy name. With no message (the Library) it is just those two.
 */
export function ImageContextMenu({ source, menu: message, copyImage, className, children }: {
  source: ImageSource
  menu?: MessageMenu
  /** What Copy image does, where something shows the result (the viewer's copy button). */
  copyImage?: () => void
  className?: string
  children: ReactNode
}) {
  const store = useStoreRef()
  const menu = useContextMenu()
  return (
    <>
      <div className={className} onContextMenu={menu.onContextMenu}>{children}</div>
      <AnchoredMenu open={menu.open} onClose={menu.close} point={menu.point} reactions={message?.reactions} items={() => [...imageCopyItems(store, source, copyImage ? 'viewer' : 'menu', copyImage), ...(message?.items() ?? [])]} />
    </>
  )
}

/** "Couldn't copy image" for a moment, when Copy image from a picture's menu failed (the viewer's copy button shows its own). */
export function ImageCopyNotice() {
  const tr = useTr()
  return useCopyFailed() ? <Text as="div" size="footnote" tone="danger" role="status" className="image-copy-notice">{tr("Couldn't copy image")}</Text> : null
}

import type { ReactNode } from 'react'
import { AnchoredMenu, useContextMenu } from '../../components/Menu'
import { useStoreRef } from '../../store/context'
import type { ImageSource } from './Attachments'
import { imageCopyItems, type MessageMenu } from './use-image-actions'

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

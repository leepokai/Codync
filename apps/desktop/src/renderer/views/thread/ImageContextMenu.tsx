import type { ReactNode } from 'react'
import { AnchoredMenu, useContextMenu } from '../../components/Overlay'
import type { ImageSource } from './Attachments'
import { useImageActions } from './use-image-actions'

/** Right-click on a picture: copy the picture, or its name. */
export function ImageContextMenu({ source, className, children }: { source: ImageSource; className?: string; children: ReactNode }) {
  const menu = useContextMenu()
  const actions = useImageActions(source)
  return (
    <>
      <div className={className} onContextMenu={menu.onContextMenu}>{children}</div>
      <AnchoredMenu
        open={menu.open}
        onClose={menu.close}
        point={menu.point}
        items={() => [
          { title: 'Copy image', icon: 'photo.on.rectangle', action: () => void actions.copyImage() },
          { title: 'Copy name', icon: 'square.on.square', action: actions.copyName },
        ]}
      />
    </>
  )
}

import { createContext } from 'react'
import { IconButton } from '../components/Controls'
import { HStack } from '../components/Stack'
import './window-bar.css'

/** Where the conversation shown puts its part of the window's bar: its bot (`title`) and its panel's toggle (`end`).
 * Both null while another page covers the conversation. */
export const WindowBarSlots = createContext<{ title: HTMLElement | null; end: HTMLElement | null }>({ title: null, end: null })

/**
 * The bar across the top of the window, which also moves it: Back and Forward through the places the window has shown
 * (null at either end), the list's toggle on a page that has a list, and at the far end what the conversation puts there.
 */
export function WindowBar({ back, forward, toggleList, setTitle, setEnd }: {
  back: (() => void) | null
  forward: (() => void) | null
  toggleList: (() => void) | null
  setTitle: (slot: HTMLElement | null) => void
  setEnd: (slot: HTMLElement | null) => void
}) {
  return (
    <header className="window-bar drag">
      <HStack gap={2} className="no-drag">
        <IconButton title="Back" icon="chevron.left" size={28} disabled={!back} onClick={() => back?.()} />
        <IconButton title="Forward" icon="chevron.right" size={28} disabled={!forward} onClick={() => forward?.()} />
        {toggleList ? <IconButton title="Toggle Sidebar" icon="sidebar.left" size={28} onClick={toggleList} /> : null}
      </HStack>
      <span className="window-bar-space" />
      <div className="window-bar-title no-drag" ref={setTitle} />
      <div className="window-bar-end no-drag" ref={setEnd} />
    </header>
  )
}

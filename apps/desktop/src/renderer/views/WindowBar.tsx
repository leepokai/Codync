import { createContext } from 'react'
import { IconButton } from '../components/Controls'
import { HStack } from '../components/Stack'
import './window-bar.css'

/** Where the page shown puts its part of the window's bar: the conversation's bot (`title`) and panel toggle (`end`),
 * the Workflows page's tabs (`start`, right after the bar's own buttons). Null while another page covers it. */
type Slots = { start: HTMLElement | null; title: HTMLElement | null; end: HTMLElement | null }
/** No part of the bar: for a page another covers, or one shown where the window's bar isn't. */
export const NO_BAR_SLOTS: Slots = { start: null, title: null, end: null }
export const WindowBarSlots = createContext<Slots>(NO_BAR_SLOTS)

/**
 * The bar across the top of the window, which also moves it: Back and Forward through the places the window has shown
 * (null at either end), the list's toggle on a page that has a list, and at the far end what the conversation puts there.
 */
export function WindowBar({ back, forward, toggleList, setStart, setTitle, setEnd }: {
  back: (() => void) | null
  forward: (() => void) | null
  toggleList: (() => void) | null
  setStart: (slot: HTMLElement | null) => void
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
      <div className="window-bar-start no-drag" ref={setStart} />
      <span className="window-bar-space" />
      <div className="window-bar-title no-drag" ref={setTitle} />
      <div className="window-bar-end no-drag" ref={setEnd} />
    </header>
  )
}

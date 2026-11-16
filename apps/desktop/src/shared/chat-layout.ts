// The chat window's two columns at their narrowest; chat-window.css repeats them (.chat-window, .detail).
export const sidebarMinWidth = 260
export const detailMinWidth = 420
// The window can't go narrower than both columns, and that must still fit a 720-point portrait display (#359).
export const chatWindowMinWidth = sidebarMinWidth + detailMinWidth
// The page's floor: the composer dock sits at its bottom, so a shorter page cuts the composer off.
// chat-window.css repeats it (.chat-window).
export const chatWindowMinHeight = 500

export interface FrameSize {
  width: number
  height: number
}

/**
 * The window size whose page still meets the floor, from one measurement of the page and the window
 * around it: the difference is the frame and menu bar the window draws. A page that already reports
 * more than the window (page zoom below 1) leaves the floor itself as the minimum.
 */
export function windowMinimumFor(page: FrameSize, frame: FrameSize): FrameSize {
  return {
    width: chatWindowMinWidth + Math.max(0, frame.width - page.width),
    height: chatWindowMinHeight + Math.max(0, frame.height - page.height),
  }
}

/** Raise a window only when it is below the minimum; one the person already made larger stays as it is. */
export function growToMinimum(size: FrameSize, minimum: FrameSize): FrameSize {
  return { width: Math.max(size.width, minimum.width), height: Math.max(size.height, minimum.height) }
}

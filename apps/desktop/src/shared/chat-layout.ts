// The chat window's two columns at their narrowest; chat-window.css repeats them (.chat-window, .detail).
export const sidebarMinWidth = 260
export const detailMinWidth = 420
// The window can't go narrower than both columns, and that must still fit a 720-point portrait display (#359).
export const chatWindowMinWidth = sidebarMinWidth + detailMinWidth
// The page's floor: the composer dock sits at its bottom, so a shorter page cuts the composer off.
// chat-window.css repeats it (.chat-window).
export const chatWindowMinHeight = 500

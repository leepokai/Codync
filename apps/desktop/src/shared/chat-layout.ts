// The chat window's two columns at their narrowest; chat-window.css repeats them (.chat-window, .detail).
export const sidebarMinWidth = 260
export const detailMinWidth = 420
// The window can't go narrower than both columns, and that must still fit a 720-point portrait display (#359).
export const chatWindowMinWidth = sidebarMinWidth + detailMinWidth

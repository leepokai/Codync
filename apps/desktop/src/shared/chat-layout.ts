// The chat window's columns at their narrowest: the rail, a page's list and what it shows. chat-window.css and
// nav-rail.css repeat them (.chat-window, .nav-rail, .detail, .page-frame).
export const railWidth = 64
// The chrome strip right of and under the page, closing the frame the bar and rail start.
export const frameInset = 8
export const sidebarMinWidth = 196
export const detailMinWidth = 420
// The window can't go narrower than the three, and that must still fit a 720-point portrait display (#359).
export const chatWindowMinWidth = railWidth + sidebarMinWidth + detailMinWidth + frameInset
// The page's floor: the composer dock sits at its bottom, so a shorter page cuts the composer off.
// chat-window.css repeats it (.chat-window).
export const chatWindowMinHeight = 500

/** The narrowest the side panel beside a conversation or a canvas goes. */
export const PANEL_MIN = 300
/** What a conversation keeps beside the side panel. */
export const CONVERSATION_MIN = 340

/** Whether the panel fits beside a view `room` wide that keeps `keep`, with room left to drag it; if not, it covers the view. */
export const fitsBeside = (room: number, keep: number) => room - keep > PANEL_MIN

/** `width` as the panel's width beside a view `room` wide that keeps `keep`: never under PANEL_MIN. */
export const fitPanel = (width: number, room: number, keep: number) => Math.max(PANEL_MIN, Math.min(width, room - keep))

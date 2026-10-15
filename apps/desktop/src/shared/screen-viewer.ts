/** What the screen window needs. The VNC port is one this app forwarded over SSH. */
export interface ScreenWindowTarget {
  computerId: string
  name: string
  vncPort: number
}

/** What the open window reads. The WebSocket is the loopback bridge to `vncPort`. */
export interface ScreenView {
  computerId: string
  name: string
  url: string
}

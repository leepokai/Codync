// Mounts an app view over the running window for the fixtures that show states a sandbox never reaches.
import type { ReactNode } from 'react'
import { createRoot } from 'react-dom/client'

/** Renders `node` in a full-window layer with `className`; returns its unmount. */
export function mountOverlay(className: string, node: ReactNode, style = '') {
  const element = document.createElement('div')
  element.className = className
  element.style.cssText = `position:fixed;inset:0;z-index:99999;background:var(--background, #fff);display:flex;${style}`
  document.body.append(element)
  const root = createRoot(element)
  root.render(node)
  return () => { root.unmount(); element.remove() }
}

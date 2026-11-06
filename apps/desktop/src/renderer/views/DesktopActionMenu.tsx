import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import type { MenuItem } from '../components/Overlay'
import { ACTION_MENU_WIDTH, menuPlacement, submenuPlacement } from '../lib/menu-placement'
import { submenuHover } from '../lib/submenu-hover'
import { useTr } from '../lib/use-language'
import { PanelRows, type PanelItem } from './PanelMenus'

/** Roster actions, with one level of desktop submenus. */
export type ActionItem = MenuItem | (Omit<MenuItem, 'action'> & { submenu: MenuItem[] })

/** Hover opens a child beside its parent. Clicking, Enter, and → enter it; ← or Escape returns. */
export function DesktopActionMenu({ items, onDismiss, x, y }: { items: ActionItem[]; onDismiss: () => void; x: number; y: number }) {
  const tr = useTr()
  const menu = useRef<HTMLDivElement>(null)
  const child = useRef<HTMLDivElement>(null)
  const anchor = useRef<HTMLButtonElement | null>(null)
  const hoveredRow = useRef<HTMLButtonElement | null>(null)
  const [open, setOpen] = useState<number | null>(null)
  const [level, setLevel] = useState<'parent' | 'child'>('parent')
  const [view, setView] = useState({ width: window.innerWidth, height: window.innerHeight })
  const [size, setSize] = useState({ parent: 0, child: 0 })
  const hover = useRef<ReturnType<typeof submenuHover> | null>(null)
  if (!hover.current) hover.current = submenuHover((index) => {
    if (index !== null) anchor.current = hoveredRow.current
    setOpen(index)
  })
  useEffect(() => () => hover.current?.cancel(), [])
  useEffect(() => {
    const resize = () => { hover.current?.cancel(); setOpen(null); setLevel('parent'); setView({ width: window.innerWidth, height: window.innerHeight }) }
    window.addEventListener('resize', resize)
    return () => window.removeEventListener('resize', resize)
  }, [])
  useLayoutEffect(() => {
    const parent = menu.current?.offsetHeight ?? 0
    const sub = child.current?.offsetHeight ?? 0
    setSize((old) => old.parent === parent && old.child === sub ? old : { parent, child: sub })
  })
  const closeChild = () => { hover.current?.cancel(); setOpen(null); setLevel('parent') }
  const returnToParent = () => { closeChild(); anchor.current?.focus({ preventScroll: true }) }
  const enter = (index: number, row: HTMLButtonElement | null) => {
    const item = items[index]
    if (!row || !item || !('submenu' in item)) return
    hover.current?.cancel()
    anchor.current = row
    setOpen(index)
    setLevel('child')
  }
  const point = (index: number, row: HTMLButtonElement) => {
    const item = items[index]
    setLevel('parent')
    if (item && 'submenu' in item) {
      hoveredRow.current = row
      hover.current?.schedule(index)
    } else hover.current?.schedule(null)
  }
  const convert = (item: ActionItem, index: number): PanelItem => ({
    title: item.title, icon: item.icon ?? 'circle', detail: item.detail, verbatim: item.verbatim, destructive: item.destructive,
    ...('submenu' in item ? { chevron: true, submenu: true, expanded: open === index, action: (row: HTMLButtonElement | null) => enter(index, row) }
      : { action: () => { onDismiss(); item.action() } }),
  })
  const branch = open === null ? null : items[open]
  const sub = branch && 'submenu' in branch ? branch : null
  const placement = menuPlacement({ x, y }, { width: ACTION_MENU_WIDTH, height: size.parent }, view)
  const subPlacement = sub && anchor.current && menu.current ? submenuPlacement(
    { ...placement, right: placement.left + placement.width, height: size.parent }, anchor.current.getBoundingClientRect(),
    { width: ACTION_MENU_WIDTH, height: size.child }, view,
  ) : null
  return <div onMouseLeave={() => hover.current?.schedule(null)} onMouseEnter={() => hover.current?.cancel()}>
    <div ref={menu} className="action-menu" style={placement}>
      <div className="action-menu-rows" onScroll={closeChild}>
        <PanelRows items={items.map(convert)} active={level === 'parent' || !sub} onDismiss={onDismiss}
          onPoint={point} onNavigate={closeChild} onForward={enter}
          after={(i) => items[i + 1]?.divider ? <div className="panel-divider" role="separator" /> : null} />
      </div>
    </div>
    {sub && subPlacement ? <div ref={child} className="action-submenu" style={subPlacement}
      onMouseEnter={() => { hover.current?.cancel(); setLevel('child') }}>
      <div className="action-menu-rows">
        <PanelRows key={open} label={sub.verbatim ? sub.title : tr(sub.title)} items={sub.submenu.map(convert)}
          active={level === 'child'} onDismiss={returnToParent} onBack={returnToParent}
          after={(i) => sub.submenu[i + 1]?.divider ? <div className="panel-divider" role="separator" /> : null} />
      </div>
    </div> : null}
  </div>
}

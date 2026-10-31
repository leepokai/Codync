import { useLayoutEffect, useRef, useState } from 'react'
import { Icon } from '../../components/Icon'
import { AnchoredMenu } from '../../components/Overlay'
import { Text } from '../../components/Text'
import type { AppLanguage } from '../../lib/l10n'
import { useAppLanguage, useTr } from '../../lib/use-language'
import { runnableNodes, type Graph, type WorkflowSummary, type WorkflowVersion } from './workflow'

/** The canvas title menu: this computer's workflows, then version history beside it. */
export function WorkflowTitleMenu({ title, selectedId, workflows, graph, versions, previewId, pinned, onOpen, onCreate, onRename, onDuplicate, onDelete, onPin, onLoadVersions, onPreview, onShowCurrent }: {
  title: string
  selectedId: string | null
  workflows: WorkflowSummary[]
  graph: Graph | null
  versions: WorkflowVersion[]
  previewId: string | null
  pinned: boolean
  onOpen: (id: string) => void
  onCreate: () => void
  onRename: () => void
  onDuplicate: () => void
  onDelete: () => void
  onPin: () => void
  onLoadVersions: () => void
  onPreview: (version: WorkflowVersion) => void
  onShowCurrent: () => void
}) {
  const tr = useTr()
  const language = useAppLanguage()
  const system = typeof navigator === 'undefined' ? 'en' : navigator.language
  const button = useRef<HTMLButtonElement>(null)
  const [open, setOpen] = useState(false)
  const [versionsOpen, setVersionsOpen] = useState(false)
  const [flip, setFlip] = useState(false)
  const steps = graph ? runnableNodes(graph).length : 0
  const close = () => { setOpen(false); setVersionsOpen(false) }
  const choose = (action: () => void) => { close(); action() }
  useLayoutEffect(() => {
    const node = button.current
    if (!open || !node) return
    setFlip(node.getBoundingClientRect().right + 300 > window.innerWidth)
  }, [open])
  return (
    <>
      <button
        ref={button}
        type="button"
        className="workflow-title"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => {
          setOpen((value) => {
            if (!value) onLoadVersions()
            return !value
          })
          setVersionsOpen(false)
        }}
      >
        <Text weight="semibold" truncate>{title || tr('Workflows')}</Text>
        <Icon name="chevron.down" size={11} weight="semibold" color="var(--secondary)" />
      </button>
      <AnchoredMenu open={open} onClose={close} anchor={button} className="workflow-title-panel">
        <div className="workflow-title-menu" role="menu">
          <Text size="caption" tone="secondary" className="workflow-menu-label">{tr('Workflows')}</Text>
          <div className="workflow-title-list">
            {workflows.map((item) => {
              const on = item.id === selectedId
              return (
                <button key={item.id} type="button" className="menu-row" role="menuitemradio" aria-checked={on} onClick={() => choose(() => onOpen(item.id))}>
                  <Mark on={on} />
                  <span className={`menu-title${on && steps > 0 ? ' with-detail beside' : ''}`}>
                    <span>{item.title}</span>
                    {on && steps > 0 ? <span className="menu-detail">{tr('{count} steps').replaceAll('{count}', String(steps))}</span> : null}
                  </span>
                </button>
              )
            })}
          </div>
          <button type="button" className="menu-row" role="menuitem" onClick={() => choose(onCreate)}>
            <span className="workflow-mark add"><Icon name="plus" size={10} /></span>
            <span className="menu-title">{tr('New workflow')}</span>
          </button>
          <div className="menu-divider" role="separator" />
          <div className="workflow-version-anchor" onMouseEnter={() => setVersionsOpen(true)} onMouseLeave={() => setVersionsOpen(false)}>
            <button type="button" className={`menu-row${versionsOpen ? ' highlight' : ''}`} role="menuitem" aria-expanded={versionsOpen} onClick={() => setVersionsOpen((value) => !value)}>
              <span className="menu-icon"><Icon name="clock" size={15} /></span>
              <span className="menu-title">{tr('Version history')}</span>
              <Icon name="chevron.right" size={12} />
            </button>
            {versionsOpen ? (
              <div className={`workflow-version-pop${flip ? ' flip' : ''}`}>
                <div className="workflow-version-menu" role="menu">
                  <button type="button" className="menu-row" role="menuitemradio" aria-checked={previewId === null} onClick={() => choose(onShowCurrent)}>
                    <Mark on={previewId === null} />
                    <span className="menu-title with-detail beside">
                      <span>{tr('Current canvas')}</span>
                      <span className="menu-detail">{tr('In effect')}</span>
                    </span>
                  </button>
                  {versions.map((version) => {
                    const on = version.id === previewId
                    return (
                      <button key={version.id} type="button" className="menu-row" role="menuitemradio" aria-checked={on} onClick={() => choose(() => onPreview(version))}>
                        <Mark on={on} />
                        <span className="menu-title with-detail beside">
                          <span>{versionWhen(version.createdAt, language, system)}</span>
                          <span className="menu-detail">{tr(versionKind(version.kind))}</span>
                        </span>
                      </button>
                    )
                  })}
                </div>
              </div>
            ) : null}
          </div>
          <button type="button" className="menu-row" role="menuitem" onClick={() => choose(onRename)}>
            <span className="menu-icon"><Icon name="pencil" size={15} /></span>
            <span className="menu-title">{tr('Rename')}</span>
          </button>
          <button type="button" className="menu-row" role="menuitem" onClick={() => choose(onDuplicate)}>
            <span className="menu-icon"><Icon name="plus.square.on.square" size={15} /></span>
            <span className="menu-title">{tr('Make a copy')}</span>
          </button>
          <button type="button" className="menu-row" role="menuitem" onClick={() => choose(onPin)}>
            <span className="menu-icon"><Icon name={pinned ? 'pin.slash' : 'pin'} size={15} /></span>
            <span className="menu-title">{tr(pinned ? 'Unpin' : 'Pin')}</span>
          </button>
          <button type="button" className="menu-row destructive" role="menuitem" onClick={() => choose(onDelete)}>
            <span className="menu-icon"><Icon name="trash" size={15} /></span>
            <span className="menu-title">{tr('Delete workflow')}</span>
          </button>
        </div>
      </AnchoredMenu>
    </>
  )
}

function Mark({ on }: { on: boolean }) {
  return <span className={`workflow-mark${on ? ' on' : ''}`}>{on ? <Icon name="checkmark" size={10} /> : null}</span>
}

function versionKind(kind: WorkflowVersion['kind']) {
  if (kind === 'run') return 'Before the run'
  if (kind === 'restore') return 'Before restoring'
  return 'Autosaved'
}

function versionWhen(at: number, language: AppLanguage, system: string) {
  const locale = language === 'zh' ? 'zh-CN' : language === 'zh-Hant' ? 'zh-TW' : language === 'system' ? system : language === 'en' ? undefined : language
  const date = new Date(at)
  const day = date.toLocaleDateString(locale, { month: 'long', day: 'numeric' })
  const time = date.toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit', hourCycle: 'h23' })
  return `${day} ${time}`
}

import { useEffect, useMemo, useState } from 'react'
import { CardSection, IconButton, Switch, TabChoice } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Sheet } from '../../components/Overlay'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useModel } from '../../lib/observable'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { useScreenViewed } from '../../store/analytics'
import { MemoryStore, authorLabel, type MemoryEntry } from '../../store/memory-store'
import { FormSheet } from '../marketplace/pieces'
import { relativeTime } from '../thread/chat-items'
import { MemoryInfoSheet, dreamsInfo, harnessInfo } from './MemoryInfoSheet'
import { type FileTarget, MemoryFileSheet, fileRows } from './MemoryFiles'
import './memory.css'

type Section = 'yours' | 'bot' | 'dreams' | 'harness'
/** What an info button explains. */
type Info = 'harness' | 'dreams'

/** A bot's memory, read-only: what all your bots know about you, what this one knows, its dreams, and its harness's own notes when the user kept them on. The user changes memory by telling the bot. */
export function MemoryCard({ botId }: { botId: string }) {
  const host = useStore()
  const tr = useTr()
  const memory = useModel(useMemo(() => new MemoryStore(botId, () => host.ready()), [botId, host]))
  useScreenViewed('memory')
  const bot = host.bots.get(botId)
  const [section, setSection] = useState<Section>('bot')
  const [file, setFile] = useState<FileTarget | null>(null)
  const [info, setInfo] = useState<Info | null>(null)
  // Memory changes anywhere move the bot's `memoryRev`: fetch again.
  useEffect(() => { void memory.load() }, [memory, bot?.memoryRev, bot?.sharedMemory, bot?.harnessMemory])

  const view = memory.view
  // Claude Code's section goes when its memory is turned off (on another device too).
  useEffect(() => { if (section === 'harness' && view && !view.projectMemory) setSection('bot') }, [section, view])
  const harness = host.backendName(bot?.backend ?? 'claude')
  const own = tr("{name}'s own memory").replaceAll('{name}', harness)
  const sections: { id: Section; label: string }[] = [
    { id: 'yours', label: 'About you' }, { id: 'bot', label: 'This bot' }, { id: 'dreams', label: 'Dreams' },
    ...(view?.projectMemory ? [{ id: 'harness' as const, label: harness }] : []),
  ]
  const layer = section === 'yours' ? view?.yours : section === 'bot' ? view?.bot : undefined

  return <>
    <div className="memory-panel">
      <TabChoice label="Memory" selection={section} options={sections} onChange={(next) => {
        setSection(next)
        // Claude Code writes its notes outside Codync, so nothing moves `memoryRev`: read them again on the way in.
        if (next === 'harness') void memory.load()
      }} />
      <CardSection>
        {memory.error && <p role="alert" className="memory-error">{memory.error}</p>}
        {!view && !memory.error && <p className="memory-muted">Loading memory…</p>}
        {view && bot && section === 'yours' && <Switch on={bot.sharedMemory} onChange={(on) => host.setSharedMemory(bot, on)}>
          Use what your bots share
        </Switch>}
        {/* The harness's own memory: a switch where Codync can turn it off, "Shared" where every bot on it shares it. */}
        {view && bot && section === 'bot' && (bot.harnessMemory !== null || bot.harnessMemoryShared) && <HStack gap={4}>
          <span>{own}</span>
          <IconButton title={tr("About {name}'s own memory").replaceAll('{name}', harness)} icon="info.circle" onClick={() => setInfo('harness')} />
          {bot.harnessMemory !== null
            ? <Switch on={bot.harnessMemory} onChange={(on) => host.setHarnessMemory(bot, on)} label={own} className="memory-harness-switch" />
            : <Text tone="secondary" className="memory-harness-shared">{tr('Shared')}</Text>}
        </HStack>}
        {layer && [
          <p key="note" className="memory-muted">{tr(section === 'yours'
            ? 'Your name, time zone, people and lasting preferences, shared by your bots. {used} of {cap} in use. To change it, tell the bot.'
            : 'What this bot learned about you. {used} of {cap} in use. To change it, tell the bot.')
            .replace('{used}', String(layer.used)).replace('{cap}', String(layer.cap))}</p>,
          ...(layer.entries.length ? [] : [<div key="empty" className="memory-empty">
            <Icon name="brain" size={20} color="var(--tertiary)" />
            <span>{tr(section === 'yours' ? 'Nothing shared yet.' : 'Nothing yet. What this bot learns about you shows up here.')}</span>
          </div>]),
          ...layer.entries.map((entry) => <EntryRow key={entry.id} entry={entry} />),
          ...(view?.files ? fileRows(view.files, section === 'yours', setFile, tr) : []),
        ]}
        {view && section === 'dreams' && [
          <div key="note" className="memory-layer-note">
            <p className="memory-muted">{tr('Between chats the bot tidies its memory from your own words.')}</p>
            <IconButton title="About dreaming" icon="info.circle" onClick={() => setInfo('dreams')} />
          </div>,
          ...(view.dreams.length ? [] : [<div key="empty" className="memory-empty"><Icon name="moon" size={20} color="var(--tertiary)" /><span>{tr('No dreams yet.')}</span></div>]),
          ...view.dreams.map((dream) => <div className="memory-content fade-in" key={dream.id}>
            <strong>{relativeTime.day(dream.at, tr)}{dream.undone ? ' · undone' : ''}</strong>
            {dream.added.map((c) => <Text key={`a${c.id}`} tone="added" className="line-clamp memory-clamp" title={c.text}>+ {c.text}</Text>)}
            {dream.archived.map((c) => <Text key={`r${c.id}`} tone="removed" className="line-clamp memory-clamp" title={c.text}>− {c.text}</Text>)}
            {(dream.files ?? []).map((f) => <span key={`f${f.version}`} className="memory-muted">Added to {f.name}</span>)}
          </div>),
        ]}
        {view?.projectMemory && section === 'harness' && <div className="memory-content">
          <p className="memory-muted">{tr("Claude Code's own notes, read-only here.")}</p>
          <small className="memory-muted">{view.projectMemory.path}</small>
          {view.projectMemory.text
            ? <pre className="memory-history">{view.projectMemory.text}</pre>
            : <div className="memory-empty"><Icon name="brain" size={20} color="var(--tertiary)" /><span>{tr("Nothing yet. Claude Code's notes show up here once it saves some.")}</span></div>}
        </div>}
      </CardSection>
    </div>
    <Sheet open={file !== null} onClose={() => setFile(null)} width={640}>
      {file && <MemoryFileSheet memory={memory} target={file} />}
    </Sheet>
    <FormSheet open={info !== null} onClose={() => setInfo(null)} height={560}>
      {info === 'harness' && <MemoryInfoSheet icon="brain" title={own}
        points={harnessInfo(harness, bot?.harnessMemoryShared ? 'shared' : bot?.backend === 'claude' ? 'readable' : 'hidden', tr)} />}
      {info === 'dreams' && <MemoryInfoSheet icon="moon" title="Dreaming" points={dreamsInfo} />}
    </FormSheet>
  </>
}

function EntryRow({ entry }: { entry: MemoryEntry }) {
  const host = useStore()
  const tr = useTr()
  const archived = entry.state === 'archived'
  return <div className="memory-content fade-in">
    <strong className={archived ? 'memory-archived' : undefined}>{entry.pinned && <Icon name="pin.fill" size={10} color="var(--secondary)" />}<span className="line-clamp memory-clamp" title={entry.text}>{entry.text}</span></strong>
    <small>{authorLabel(entry, (id) => host.authorName(id))} · {relativeTime.day(entry.updatedAt, tr)}{archived ? ' · archived' : ''}</small>
  </div>
}

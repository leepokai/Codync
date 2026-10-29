import React, { useState } from 'react'
import { createRoot } from 'react-dom/client'
import { ChatSidePanel } from '../../src/renderer/views/thread/ChatSidePanel'
import { Composer } from '../../src/renderer/views/thread/Composer'
import { StoreContext } from '../../src/renderer/store/context'
import { BotStore } from '../../src/renderer/store/bot-store'
import { normalizeBot, type Entry } from '../../src/shared/models'
import '../../src/renderer/styles/theme.css'
import '../../src/renderer/views/thread/thread.css'
import '../../src/renderer/views/thread/chat-controls.css'

// Actual store and component, with no connection to the user's host or account.
const store = new BotStore({ id: 'composer-check', name: 'Fixture', signKey: '', urls: [] }, true,
  () => { throw new Error('Fixture must not connect to a host') }, 'mac', '2.7.0', 'composer-check')
store.connection = { kind: 'online' }
for (const id of ['first', 'second', 'group']) store.bots.set(id, normalizeBot({ id, name: id, kind: id === 'group' ? 'group' : 'agent', members: ['first', 'second'] }))
const sent: unknown[] = []
store.send = (text, botId, { files, replyTo } = {}) => { sent.push({ text, botId, replyTo: replyTo?.id, files: files?.length }); renderCount(n => n + 1) }
store.stop = () => { store.bots.get('first')!.status = 'idle'; renderCount(n => n + 1) }
let renderCount: React.Dispatch<React.SetStateAction<number>> = () => {}
const quoted: Entry = { id: 'root', seq: 1, botId: 'first', rev: 1, kind: 'agent', turn: 1, createdAt: 0, updatedAt: 0, data: { text: 'A long earlier reply that the quote bar cuts off at the end of one line', final: true, author: 'first' } }
function Check() {
  const [bot, setBot] = useState('first')
  const [reply, setReply] = useState<Entry | null>(null)
  const [mounted, setMounted] = useState(true)
  const [calling, setCalling] = useState(false)
  const [panel, setPanel] = useState(false)
  const [width, setWidth] = useState(620)
  const [, update] = useState(0)
  renderCount = update
  return <StoreContext.Provider value={store}>
    <main style={{ padding: 32, '--conversation': '16px' } as React.CSSProperties}>
      <h1>Composer regression checks</h1>
      <nav style={{ display: 'flex', gap: 18, margin: '24px 0' }}>
        {['first', 'second', 'group'].map(id => <button key={id} onClick={() => setBot(id)}>{id}</button>)}
        <button onClick={() => setReply(r => r ? null : quoted)}>Toggle reply</button>
        <button onClick={() => setMounted(m => !m)}>Toggle composer</button>
        <button onClick={() => setPanel(p => !p)}>Toggle panel</button>
        <button onClick={() => setWidth(w => w === 620 ? 300 : 620)}>Resize</button>
        <button onClick={() => { const b = store.bots.get(bot)!; b.status = b.status === 'working' ? 'idle' : 'working'; update(n => n + 1) }}>Toggle working</button>
        <button onClick={() => { store.connection = store.connection.kind === 'online' ? { kind: 'offline', message: 'fixture' } : { kind: 'online' }; update(n => n + 1) }}>Toggle offline</button>
      </nav>
      <p>{bot} / {reply ? 'replying' : 'new message'} / {calling ? 'calling' : 'silent'}</p>
      <div style={{ width, paddingTop: 20 }}>
        {mounted ? <Composer botId={bot} reply={reply} cancelReply={() => setReply(null)} onCall={() => setCalling(true)} onInterrupt={calling ? () => setCalling(false) : null} /> : null}
      </div>
      <div style={{ display: 'flex', height: 100 }}><ChatSidePanel open={panel} width={292}><p>Details fixture</p></ChatSidePanel></div>
      <output aria-label="Sent messages">{JSON.stringify(sent)}</output>
    </main>
  </StoreContext.Provider>
}
createRoot(document.getElementById('root')!).render(<Check />)

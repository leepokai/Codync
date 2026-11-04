import { useEffect, useMemo, useState } from 'react'
import { isGroup, type Bot } from '@shared/models'
import { Button, DropdownMenu, SegmentedChoice, Spinner } from '../../../components/Controls'
import { AnchoredMenu } from '../../../components/Overlay'
import { HStack, Spacer, VStack } from '../../../components/Stack'
import { Text } from '../../../components/Text'
import { useModel } from '../../../lib/observable'
import { NODE_WIDTH, canvasBotIds, connectProblem, layoutNodes, type FlowCanvas, type FlowLink } from '../../../lib/routine-flow.ts'
import { useTr } from '../../../lib/use-language'
import { useStore } from '../../../store/context'
import { RoutineFlowStore } from '../../../store/routine-flow-store'
import { FlowCanvasView, type FlowSelection } from './FlowCanvas'
import { FlowInspector } from './FlowInspector'
import { FlowList } from './FlowList'
import './routine-flow.css'

type Mode = 'canvas' | 'list'

/** The bots a routine flow can hand work to: agents of their own, not hidden. */
const canTakeWork = (bot: Bot) => !bot.hidden && !isGroup(bot) && bot.backend !== 'external'

/** Where a bot added by hand lands: right of everything on the canvas. */
function freeSpot(canvas: FlowCanvas) {
  const nodes = layoutNodes(canvas)
  return { x: Math.max(0, ...nodes.map((n) => n.x + NODE_WIDTH + 90)), y: Math.min(0, ...nodes.map((n) => n.y)) }
}

/** A bot's routine flow: its canvas (or the same flow as a list), and an inspector for the selection. */
export function RoutineFlowView({ botId }: { botId: string }) {
  const tr = useTr()
  const store = useStore()
  const [flow] = useState(() => new RoutineFlowStore(botId, () => store.ready()))
  const model = useModel(flow)
  const [mode, setMode] = useState<Mode>('canvas')
  const [selection, setSelection] = useState<FlowSelection>(null)
  const [note, setNote] = useState<string | null>(null)
  const [choose, setChoose] = useState<{ from: string; to: string; at: { x: number; y: number }; routines: { id: string; name: string }[] } | null>(null)
  const offline = store.isOffline
  const rev = store.flowRev(botId)

  useEffect(() => {
    flow.resume()
    void flow.load()
    return () => flow.dispose()
  }, [flow])
  // The host says the canvas changed (another client, a step finishing): load it again.
  useEffect(() => {
    if (rev > 0) void flow.load()
  }, [rev, flow])

  const canvas = model.canvas
  const bots = store.bots
  const roster = useMemo(() => [...bots.values()].filter(canTakeWork), [bots])

  const connectable = (c: FlowCanvas, from: string, to: string) =>
    c.routines.filter((r) => connectProblem(c, r.routine.id, from, to) === null)

  const connect = async (routineId: string, from: string, to: string) => {
    setNote(null)
    const link = await flow.connect(routineId, from, to, '')
    if (link) setSelection({ kind: 'edge', id: link.id })
  }

  const dropped = (from: string, to: string, at: { x: number; y: number }) => {
    if (!canvas || offline) return
    const routines = connectable(canvas, from, to)
    if (routines.length === 0) {
      const reason = canvas.routines.map((r) => connectProblem(canvas, r.routine.id, from, to)).find(Boolean)
      setNote(tr(reason ?? 'These bots can\'t be connected.'))
    } else if (routines.length === 1) {
      void connect(routines[0].routine.id, from, to)
    } else {
      setChoose({ from, to, at, routines: routines.map((r) => ({ id: r.routine.id, name: r.routine.name })) })
    }
  }

  const targets = (from: string): Bot[] =>
    canvas ? roster.filter((bot) => bot.id !== from && connectable(canvas, from, bot.id).length > 0) : []

  if (flow.unsupported) {
    return <Text size="callout" tone="secondary" className="outlined-card flow-state">{tr('This computer\'s host doesn\'t support routine flows yet. Update Codync on it.')}</Text>
  }
  if (!canvas) {
    return model.error ? (
      <VStack gap={8} className="outlined-card flow-state">
        <Text size="callout" tone="danger">{model.error}</Text>
        <Button kind="secondary" onClick={() => void flow.load()}>{tr('Try again')}</Button>
      </VStack>
    ) : (
      <HStack gap={8} className="outlined-card flow-state"><Spinner /><Text size="callout" tone="secondary">{tr('Loading the flow…')}</Text></HStack>
    )
  }

  const onCanvas = new Set(canvasBotIds(canvas))
  const addable = roster.filter((bot) => !onCanvas.has(bot.id))
  const addBot = (bot: Bot) => {
    const spot = freeSpot(canvas)
    void flow.place(bot.id, spot.x, spot.y)
  }
  const saveInstruction = (link: FlowLink, text: string) => void flow.updateLink(link.id, text)
  const disconnect = async (link: FlowLink) => {
    setSelection(null)
    await flow.disconnect(link.id)
  }

  return (
    <VStack gap={8} className="routine-flow">
      <HStack gap={8} wrap>
        <SegmentedChoice selection={mode} options={[{ id: 'canvas', label: 'Canvas' }, { id: 'list', label: 'List' }]} onChange={setMode} />
        <DropdownMenu
          title={tr('Add a bot to the canvas')}
          className="flow-menu-button"
          items={() => addable.map((bot) => ({ title: bot.name, action: () => addBot(bot) }))}
        >
          <Text>{addable.length === 0 ? tr('Every bot is on the canvas') : tr('Add a bot')}</Text>
        </DropdownMenu>
        <Spacer />
        {model.busy || model.loading ? <Spinner /> : null}
      </HStack>
      {offline ? <Text size="caption" tone="attention">{tr('Offline: the flow is read-only until the computer is reachable.')}</Text> : null}
      {model.error ? <Text size="caption" tone="danger">{model.error}</Text> : null}
      {note ? <Text size="caption" tone="danger">{note}</Text> : null}
      {canvas.routines.length === 0 ? (
        <Text as="div" size="callout" tone="secondary" className="outlined-card flow-state">{tr('No routines yet. Ask the bot for one, then connect it to other bots here.')}</Text>
      ) : (
        <HStack gap={8} align="stretch" className="flow-body">
          <div className="flow-main">
            {mode === 'canvas' ? (
              <FlowCanvasView
                canvas={canvas}
                bots={bots}
                selection={selection}
                disabled={offline || model.busy}
                onSelect={setSelection}
                onMove={(key, x, y) => void flow.move(key, x, y)}
                onConnect={dropped}
              />
            ) : (
              <FlowList canvas={canvas} bots={bots} selection={selection} onSelect={setSelection} />
            )}
          </div>
          <FlowInspector
            canvas={canvas}
            bots={bots}
            selection={selection}
            busy={model.busy}
            offline={offline}
            targets={targets}
            onSaveInstruction={saveInstruction}
            onDisconnect={(link) => void disconnect(link)}
            onTest={(id) => void flow.test(id)}
            onSetEnabled={(id, on) => void flow.setEnabled(id, on)}
            // No pointer position here: a routine choice, if needed, opens in the middle of the window.
            onConnectFrom={(from, to) => dropped(from, to, { x: window.innerWidth / 2, y: window.innerHeight / 2 })}
            onRemoveNode={(key) => void flow.removeNode(key)}
          />
        </HStack>
      )}
      <AnchoredMenu
        open={choose !== null}
        onClose={() => setChoose(null)}
        point={choose?.at ?? null}
        items={() => (choose?.routines ?? []).map((r) => ({ title: r.name, action: () => void connect(r.id, choose!.from, choose!.to) }))}
      />
    </VStack>
  )
}

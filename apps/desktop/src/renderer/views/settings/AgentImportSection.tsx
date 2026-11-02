import { useState } from 'react'
import { Button, CardForm, CardSection, Switch } from '../../components/Controls'
import { ModalHeader, SheetPage } from '../../components/Overlay'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useModel } from '../../lib/observable'
import { chosen, initialTicks, instructionText, memoryFacts, planImport, skippedText, type ImportItem, type ImportPlan } from '../../../shared/agent-import'
import { useApp } from '../../store/context'
import { fillDefaults, newDraft, normalized } from '../bots/drafts'
import { FormSheet, Field } from '../marketplace/pieces'
import { errorText } from './parts'

/** Settings → Import an agent: another agent's folder (OpenClaw, Hermes, or any with an AGENTS.md) becomes a bot. */
export function AgentImportSection() {
  const app = useApp()
  const local = useModel(app.local)
  const [plan, setPlan] = useState<ImportPlan | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [truncated, setTruncated] = useState(false)
  const pick = async () => {
    setError(null)
    try {
      const folder = await window.codync.agentImport.pick()
      if (!folder) return
      const found = planImport(folder.files, folder.secretPaths)
      if (!found) { setError('No OpenClaw, Hermes or AGENTS.md folder found there.'); return }
      setTruncated(folder.truncated)
      setPlan(found)
    } catch (e) { setError(errorText(e)) }
  }
  return <CardSection title="Import an agent" footer="Brings the instructions, memory and skills of an OpenClaw or Hermes agent (or any folder with an AGENTS.md) into a new bot. Settings and secret files are never read, and anything shaped like a key is left out.">
    <Button kind="secondary" disabled={!local} onClick={() => void pick()}>Choose a folder…</Button>
    {error ? <Text size="footnote" tone="danger">{error}</Text> : null}
    <FormSheet open={plan !== null} onClose={() => setPlan(null)}>
      {plan ? <ImportPreview plan={plan} truncated={truncated} done={() => setPlan(null)} /> : null}
    </FormSheet>
  </CardSection>
}

function ImportPreview({ plan, truncated, done }: { plan: ImportPlan; truncated: boolean; done: () => void }) {
  const app = useApp()
  const local = useModel(app.local)
  const [name, setName] = useState(plan.name)
  const [ticks, setTicks] = useState(() => initialTicks(plan))
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [report, setReport] = useState<string | null>(null)
  const toggle = (id: string, on: boolean) => setTicks((t) => { const next = new Set(t); if (on) next.add(id); else next.delete(id); return next })
  const picked = chosen(plan, ticks)

  const run = async () => {
    if (!local || busy) return
    setBusy(true); setError(null)
    try {
      const draft = normalized(fillDefaults(local, { ...newDraft(), name: name.trim(), description: instructionText(picked.instructions) }))
      const bot = await local.save(draft)
      const client = await local.ready()
      const facts = memoryFacts(picked.memory)
      let note = `Created ${bot.name}.`
      if (facts.length) {
        const result = await client.call<{ added: number; omitted: number; stopped: string | null }>('importAgentMemory', { botId: bot.id, facts })
        note += ` ${result.added} memories added${result.omitted ? `, ${result.omitted} left out` : ''}${result.stopped ? ` (stopped: ${result.stopped})` : ''}.`
      }
      for (const skill of picked.skills) await client.call('installSkill', { name: skill.name, description: skill.description, instructions: skill.text })
      if (picked.skills.length) note += ` ${picked.skills.length} skills installed.`
      setReport(note)
    } catch (e) { setError(errorText(e)) } finally { setBusy(false) }
  }

  return <SheetPage>
    <ModalHeader icon="square.and.arrow.down" title="Import an agent" />
    <CardForm>
      <CardSection title="Bot">
        <Field label="Name" value={name} onChange={setName} placeholder="Name" />
        <Text size="footnote" tone="secondary">Read as {plan.source === 'agents' ? 'an AGENTS.md folder' : plan.source === 'hermes' ? 'a Hermes agent' : 'an OpenClaw agent'}.</Text>
        {truncated ? <Text size="footnote" tone="attention">Some files were too big or too many and were left out.</Text> : null}
      </CardSection>
      <Group title="Instructions" items={plan.instructions} ticks={ticks} toggle={toggle} />
      <Group title="Memory" items={plan.memory} ticks={ticks} toggle={toggle} />
      <Group title="Skills" items={plan.skills} ticks={ticks} toggle={toggle} />
      {plan.skipped.length ? <CardSection title="Not imported">
        {plan.skipped.map((s) => <VStack key={`${s.path}:${s.reason}`} gap={2}><Text truncate>{s.path}</Text><Text size="caption" tone="tertiary">{skippedText[s.reason]}</Text></VStack>)}
      </CardSection> : null}
      {error ? <Text size="footnote" tone="danger">{error}</Text> : null}
      {report ? <Text role="status">{report}</Text> : null}
      {report
        ? <Button onClick={done}>Done</Button>
        : <Button disabled={busy || name.trim() === '' || !local} onClick={() => void run()}>{busy ? 'Importing…' : 'Import'}</Button>}
    </CardForm>
  </SheetPage>
}

function Group({ title, items, ticks, toggle }: { title: string; items: ImportItem[]; ticks: ReadonlySet<string>; toggle: (id: string, on: boolean) => void }) {
  if (items.length === 0) return null
  return <CardSection title={title}>
    {items.map((i) => <VStack key={i.id} gap={2}>
      <Switch on={ticks.has(i.id) && !i.credential} disabled={i.credential} onChange={(on) => toggle(i.id, on)} label={i.title}>{i.title}</Switch>
      {i.credential
        ? <Text size="caption" tone="attention">Omitted: it looks like a key or a token.</Text>
        : <Text size="caption" tone="tertiary" truncate>{i.stock ? 'An unchanged template. ' : ''}{i.text.slice(0, 160)}</Text>}
    </VStack>)}
  </CardSection>
}

import { useState } from 'react'
import { BotImport, type ImportPlan, type ImportResult } from '../../store/bot-import'
import type { BotStore } from '../../store/bot-store'
import { useModel } from '../../lib/observable'
import { useTr } from '../../lib/use-language'
import { Button, CardForm, CardSection, RowLabel, Spinner, Switch } from '../../components/Controls'
import { ModalHeader, Sheet, useDismiss } from '../../components/Overlay'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import './bots.css'

/** The import as a sheet. `store` is this computer's host: the export file is on this computer, so its bots go there. */
export function ImportBotsSheet({ store, open, onClose, maxHeight }: {
  store: BotStore | null
  open: boolean
  onClose: () => void
  maxHeight?: number
}) {
  // A fresh import each time the sheet opens.
  const [model, setModel] = useState<BotImport | null>(null)
  if (open && store && !model) setModel(new BotImport(store))
  if (!open && model) setModel(null)
  useModel(model)
  // The host keeps importing after the sheet closes, so it stays up until the import ends.
  return (
    <Sheet open={open && !!model} onClose={model?.busy ? () => {} : onClose} maxHeight={maxHeight}>
      {open && model ? <ImportBotsView model={model} /> : null}
    </Sheet>
  )
}

/** Bots from another app: the export steps, the bots and group chats in the file, then what came over. */
function ImportBotsView({ model }: { model: BotImport }) {
  const step = model.step

  return (
    <VStack className="import-sheet">
      <ModalHeader icon="square.and.arrow.down" title="Import bots" subtitle="Bring your agents over from Grok Bot" />
      <VStack gap={16} className="import-body">
        {step.kind === 'start' ? <Start model={model} /> : null}
        {step.kind === 'review' ? <Review model={model} plan={step.plan} skipped={step.skipped} /> : null}
        {step.kind === 'done' ? <Done result={step.result} /> : null}
        {model.error ? <Text size="footnote" tone="danger" className="fade-in">{model.error}</Text> : null}
      </VStack>
      <Footer model={model} />
    </VStack>
  )
}

function Start({ model }: { model: BotImport }) {
  const tr = useTr()
  return (
    <CardForm>
      <CardSection title="How it works" footer="Chats aren’t copied: the export keeps what matters in them as memories. Exports other apps make from Grok Bot work too.">
        <RowLabel label="1. Add the Codync export skill to an agent in Grok Bot" detail="Copy it, then paste it as a new skill or into the agent’s instructions." />
        <RowLabel label={`2. ${tr(`Send it: “Export my agents for Codync”`)}`} detail="It saves one .zip file to Downloads." />
        <RowLabel label="3. Choose that file here" detail="Read on this computer; nothing is uploaded." />
      </CardSection>
      <HStack gap={8}>
        <Button kind="secondary" onClick={() => void model.copySkill()} disabled={model.busy}>{model.copied ? 'Copied' : 'Copy export skill'}</Button>
      </HStack>
    </CardForm>
  )
}

function Review({ model, plan, skipped }: { model: BotImport; plan: ImportPlan; skipped: string[] }) {
  const tr = useTr()
  const notes = [...plan.warnings, ...skipped.map((path) => `${tr(`Left out`)} ${path}`)]
  return (
    <CardForm>
      <CardSection title="Bots">
        {plan.bots.map((bot) => (
          <Switch key={bot.key} on={model.picked('bot', bot.key)} onChange={() => model.toggle('bot', bot.key)}>
            <RowLabel label={bot.name} detail={summary(tr, bot.skills.length, bot.routines.length, bot.memories.length, bot.files !== null)} />
          </Switch>
        ))}
      </CardSection>
      {plan.groups.length ? (
        <CardSection title="Group chats">
          {plan.groups.map((group) => (
            <Switch key={group.key} on={model.picked('group', group.key) && model.groupAvailable(group.members)} disabled={!model.groupAvailable(group.members)} onChange={() => model.toggle('group', group.key)}>
              <RowLabel label={group.name} detail={members(plan, group.members)} />
            </Switch>
          ))}
        </CardSection>
      ) : null}
      {notes.length ? (
        <CardSection title="Notes">
          {notes.map((note) => <Text key={note} size="caption" tone="secondary">{note}</Text>)}
        </CardSection>
      ) : null}
    </CardForm>
  )
}

function Done({ result }: { result: ImportResult }) {
  const tr = useTr()
  return (
    <CardForm>
      <CardSection title="Imported">
        {result.bots.map((bot) => <RowLabel key={bot.id} label={bot.name} detail={summary(tr, bot.skills, bot.routines, bot.memories, bot.files > 0)} />)}
        {result.groups.map((group) => <RowLabel key={group.id} label={group.name} detail="Group chat" />)}
      </CardSection>
      {result.warnings.length ? (
        <CardSection title="Notes">
          {result.warnings.map((note) => <Text key={note} size="caption" tone="secondary">{note}</Text>)}
        </CardSection>
      ) : null}
    </CardForm>
  )
}

function Footer({ model }: { model: BotImport }) {
  const dismiss = useDismiss()
  const step = model.step
  return (
    <HStack gap={8} className="create-footer">
      {model.busy ? <Spinner /> : null}
      <Spacer />
      {step.kind === 'done' ? (
        <Button onClick={dismiss}>Done</Button>
      ) : (
        <>
          <Button kind="secondary" onClick={dismiss} disabled={model.busy}>Cancel</Button>
          {step.kind === 'start' ? <Button onClick={() => void model.choose()} disabled={model.busy}>Choose file…</Button> : null}
          {step.kind === 'review' ? <Button onClick={() => void model.importPicked()} disabled={!model.canImport}>Import</Button> : null}
        </>
      )}
    </HStack>
  )
}

function summary(tr: (s: string) => string, skills: number, routines: number, memories: number, files: boolean) {
  const parts = [`${skills} ${tr(`skills`)}`, `${routines} ${tr(`routines`)}`, `${memories} ${tr(`memories`)}`]
  if (files) parts.push(tr(`workspace files`))
  return parts.join(' · ')
}

function members(plan: ImportPlan, keys: string[]) {
  return keys.map((key) => plan.bots.find((b) => b.key === key)?.name ?? key).join(', ')
}

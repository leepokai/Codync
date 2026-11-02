import { useCallback, useEffect, useState } from 'react'
import { Button, CardSection } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { approvable, inboxTitle, stepsPreview, toolsText, type SkillProposal } from '../../../shared/skill-proposals'
import { useStore } from '../../store/context'
import { Field } from '../marketplace/pieces'

/** The skills a bot proposes after a procedure that used several kinds of tool: one card each, nothing is a skill until approved. */
export function SkillProposalsCard({ botId }: { botId: string }) {
  const host = useStore()
  const tr = useTr()
  const [proposals, setProposals] = useState<SkillProposal[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [editing, setEditing] = useState<string | null>(null)
  const call = useCallback(async (method: string, extra: Record<string, unknown> = {}) => {
    try {
      const reply = await (await host.ready()).call<{ proposals: SkillProposal[] }>(method, { botId, ...extra })
      setProposals(reply.proposals)
      setError(null)
    } catch (e) { setError(e instanceof Error ? e.message : String(e)) }
  }, [host, botId])
  useEffect(() => { void call('skillProposals') }, [call])
  if (proposals === null && error === null) return null
  const list = proposals ?? []
  return <CardSection title={inboxTitle(list.length)} footer="After a task that used several kinds of tool, the bot drafts a skill from it. It stays here until you approve it.">
    {error ? <Text size="footnote" tone="danger">{error}</Text> : null}
    {list.length === 0 && !error ? <Text tone="secondary">{tr('No proposals.')}</Text> : null}
    {list.map((p) => editing === p.id
      ? <ProposalEditor key={p.id} proposal={p} cancel={() => setEditing(null)} save={async (fields) => { await call('editSkillProposal', { id: p.id, ...fields }); setEditing(null) }} />
      : <VStack key={p.id} gap={4}>
        <Text>{p.name}</Text>
        <Text size="footnote" tone="secondary">{p.description}</Text>
        {stepsPreview(p.instructions).map((s) => <Text key={s} size="caption" tone="tertiary" truncate>{s}</Text>)}
        <Text size="caption" tone="tertiary">{tr('Used')}: {toolsText(p.tools)}</Text>
        <HStack gap={8}>
          <Button disabled={!approvable(p)} onClick={() => void call('approveSkillProposal', { id: p.id })}>Approve</Button>
          <Button kind="secondary" onClick={() => setEditing(p.id)}>Edit</Button>
          <Button kind="secondary" onClick={() => void call('dismissSkillProposal', { id: p.id })}>Dismiss</Button>
        </HStack>
      </VStack>)}
  </CardSection>
}

function ProposalEditor({ proposal, save, cancel }: { proposal: SkillProposal; save: (fields: { name: string; description: string; instructions: string }) => Promise<void>; cancel: () => void }) {
  const [name, setName] = useState(proposal.name)
  const [description, setDescription] = useState(proposal.description)
  const [instructions, setInstructions] = useState(proposal.instructions)
  return <VStack gap={6}>
    <Field label="Name" value={name} onChange={setName} placeholder="Name" />
    <Field label="When to use it" value={description} onChange={setDescription} placeholder="When to use it" lines={[2, 4]} />
    <Field label="Steps" value={instructions} onChange={setInstructions} placeholder="Steps" lines={[4, 10]} />
    <HStack gap={8}>
      <Button disabled={!approvable({ name, description, instructions })} onClick={() => void save({ name, description, instructions })}>Save</Button>
      <Button kind="secondary" onClick={cancel}>Cancel</Button>
    </HStack>
  </VStack>
}

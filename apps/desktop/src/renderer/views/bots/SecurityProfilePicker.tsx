import { useEffect, useState } from 'react'
import { ChoicePicker } from '../../components/Controls'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { previewLines, type SecurityProfile } from '@shared/security-profiles'
import { Field } from './parts'

/**
 * The security profile of a bot, chosen when it is made or later: one sentence per profile and what choosing it sets. A
 * bot with its own rules shows "Custom". The host writes the rules; this only lists the choices it offers.
 */
export function SecurityProfilePicker({ selection, saved, load, onChange }: {
  /** The profile in the form: its id, or null for a bot with its own rules. */
  selection: string | null
  /** The profile the bot has now; a different pick will replace its rules and Approvals setting. */
  saved: string | null
  load: () => Promise<SecurityProfile[] | null>
  onChange: (profile: SecurityProfile | null) => void
}) {
  const [profiles, setProfiles] = useState<SecurityProfile[] | null>(null)
  useEffect(() => {
    let live = true
    void load().then((list) => live && setProfiles(list))
    return () => { live = false }
  }, [load])
  // An older host has no profiles: nothing to choose.
  if (!profiles || profiles.length === 0) return null
  const chosen = profiles.find((p) => p.id === selection) ?? null
  const replaces = selection !== null && selection !== saved
  return (
    <Field label="Security profile" detail="How much this bot may do without asking. You can still edit the rules below.">
      <VStack gap={6}>
        <ChoicePicker
          selection={selection ?? ''}
          options={[{ id: '', label: 'Custom rules' }, ...profiles.map((p) => ({ id: p.id, label: p.title }))]}
          onChange={(id) => onChange(profiles.find((p) => p.id === id) ?? null)}
        />
        {chosen ? <Text size="footnote" tone="secondary">{chosen.summary}</Text> : null}
        {chosen ? previewLines(chosen).map((line) => <Text key={line} size="caption" tone="tertiary">{line}</Text>) : null}
        {replaces ? <Text size="footnote" tone="attention">Saving replaces this bot's Approvals setting and rules with the profile's.</Text> : null}
      </VStack>
    </Field>
  )
}

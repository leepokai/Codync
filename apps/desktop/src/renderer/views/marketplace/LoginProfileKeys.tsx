import { useState } from 'react'
import type { Backend } from '@shared/models'
import { CardForm, CardSection, IconButton, Spinner, Switch } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { SheetPage } from '../../components/Overlay'
import { useStore } from '../../store/context'
import { fill } from '../../lib/fill.ts'
import { keyRows, keyVars, newKeyRow, type KeyRow } from '../../lib/login-profile-keys'
import { useTr } from '../../lib/use-language'
import { ErrorText, Field, OptionRow, ScreenHeader, useBusyAction } from './pieces'

/**
 * An API key profile's variables: values are typed here or copied from the login shell, and kept on the
 * computer, which never sends them back. Saving or importing (which saves what was typed too) goes back to the
 * agent's sheet.
 */
export function LoginProfileKeys({ backend, loginProfile, back }: { backend: Backend; loginProfile: string; back: () => void }) {
  const tr = useTr()
  const store = useStore()
  const profile = backend.loginProfiles?.find((p) => p.id === loginProfile)
  const [rows, setRows] = useState<KeyRow[]>(() => keyRows(profile ?? {}, backend.loginKeys ?? []))
  const [removed, setRemoved] = useState<string[]>([])
  // The login shell's variable names while they're picked from; null on the form itself.
  const [shell, setShell] = useState<string[] | null>(null)
  const [ticked, setTicked] = useState<string[]>([])
  const { busy: running, error, run } = useBusyAction()
  const busy = running !== null
  const vars = keyVars(rows, removed)
  const save = (imported: string[] = []) => run('save', async () => {
    await store.setLoginProfileKeys(loginProfile, vars, imported)
    back()
  })
  const pick = () => run('pick', async () => {
    setTicked([])
    setShell(await store.loginShellVariables())
  })
  const edit = (id: number, change: Partial<KeyRow>) => setRows((old) => old.map((r) => (r.id === id ? { ...r, ...change } : r)))
  const drop = (row: KeyRow) => {
    setRows((old) => old.filter((r) => r.id !== row.id))
    if (row.saved) setRemoved((old) => [...old, row.name])
  }
  const tick = (name: string, on: boolean) => setTicked((old) => (on ? [...old, name] : old.filter((n) => n !== name)))
  const failed = error ? <CardSection><ErrorText text={error} /></CardSection> : null

  if (shell) {
    return (
      <SheetPage>
        <ScreenHeader
          title="Import from login shell"
          onBack={() => setShell(null)}
          trailing={busy ? <Spinner /> : <IconButton title="Import" icon="checkmark" disabled={!ticked.length} onClick={() => void save(ticked)} />}
        />
        <CardForm>
          <VStack gap={6}>
            <Text className="agent-keys-notes">{fill(tr('Pick the variables this login profile should use. Their values are copied now and kept on {name}.'), { name: store.hostName })}</Text>
            {shell.length ? (
              <CardSection>
                {shell.map((name) => (
                  <Switch key={name} on={ticked.includes(name)} onChange={(on) => tick(name, on)}>
                    <Text mono tone="text" className="selectable">{name}</Text>
                  </Switch>
                ))}
              </CardSection>
            ) : null}
          </VStack>
          {failed}
        </CardForm>
      </SheetPage>
    )
  }

  return (
    <SheetPage>
      <ScreenHeader
        title={profile?.name ?? 'API key'}
        onBack={back}
        trailing={busy ? <Spinner /> : <IconButton title="Save" icon="checkmark" disabled={!Object.keys(vars).length} onClick={() => void save()} />}
      />
      <CardForm>
        <VStack gap={6}>
          <CardSection>
            {rows.map((row) => (
              <HStack key={row.id} gap={8} className="login-profile-key">
                <VStack gap={4} grow>
                  {row.typed
                    ? <Field label="Variable name" placeholder="Variable name" mono value={row.name} onChange={(name) => edit(row.id, { name })} />
                    : <Text size="caption" mono tone="secondary" className="selectable">{row.name}</Text>}
                  <Field label="Value" placeholder={row.saved ? 'Saved (type to replace)' : 'Value'} secret value={row.value} onChange={(value) => edit(row.id, { value })} />
                </VStack>
                <IconButton title="Remove" icon="trash" onClick={() => drop(row)} />
              </HStack>
            ))}
            <OptionRow icon="plus" title={tr('Add variable')} disabled={busy} onClick={() => setRows((old) => [...old, newKeyRow(Math.max(-1, ...old.map((r) => r.id)) + 1)])} />
            <OptionRow icon="square.and.arrow.down" title={tr('Import from login shell')} disabled={busy} onClick={() => void pick()} />
          </CardSection>
          <Text className="agent-keys-notes">{fill(tr('Values are kept on {name} only and never shown again.'), { name: store.hostName })}</Text>
        </VStack>
        {failed}
      </CardForm>
    </SheetPage>
  )
}

import { useEffect, useMemo, useState } from 'react'
import { Button, CardSection } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { githubTokenOk } from '../../../shared/github'
import { useTr } from '../../lib/use-language'
import { useModel } from '../../lib/observable'
import { useApp } from '../../store/context'
import { GitHubStore } from '../../store/github-store'
import { ErrorText, Field } from '../marketplace/pieces'

/** The GitHub account workflows comment, label and open issues with. */
export function GitHubSection() {
  const tr = useTr()
  const app = useApp()
  const local = app.local
  const model = useModel(useMemo(() => new GitHubStore(async () => {
    if (!local) throw new Error('Start this computer’s host to connect GitHub.')
    return local.ready()
  }), [local]))
  const [token, setToken] = useState('')
  useEffect(() => { model.resume(); void model.load(); return () => model.dispose() }, [model])
  const connect = async () => { if (await model.connect(token.trim())) setToken('') }
  return (
    <CardSection
      title="GitHub"
      footer="A fine-grained token with the repository permissions your workflows need. It stays in this computer's credential vault; Codync never sends it anywhere else."
    >
      {model.status?.configured ? (
        <VStack gap={4} className="settings-stack">
          <Text>{tr('Signed in as')} {model.status.login}</Text>
          <Text size="caption" tone="tertiary">
            {model.status.scopes.length > 0 ? `${tr('Scopes')}: ${model.status.scopes.join(', ')}` : tr('No scopes reported')}
          </Text>
          <Button kind="secondary" disabled={model.busy} onClick={() => void model.disconnect()}>{tr('Disconnect GitHub')}</Button>
        </VStack>
      ) : (
        <VStack gap={6} className="settings-stack">
          <Field secret value={token} onChange={setToken} placeholder="github_pat_…" />
          <HStack>
            <Button disabled={!githubTokenOk(token) || model.busy} onClick={() => void connect()}>
              {model.busy ? tr('Checking…') : tr('Connect GitHub')}
            </Button>
          </HStack>
        </VStack>
      )}
      {model.error ? <ErrorText text={model.error} /> : null}
    </CardSection>
  )
}

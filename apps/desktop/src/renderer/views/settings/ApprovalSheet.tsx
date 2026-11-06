import { useState } from 'react'
import { Button, Reveal, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import type { Approval } from '../../store/app-model'
import { useApp } from '../../store/context'
import { deviceIcon, errorText, spacedCode } from './parts'
import { useTr } from '../../lib/use-language'


/**
 * A device asks for access through the account (spec §4.2 B). The code is the only thing that
 * tells a real request from a cloud swapping keys, so it's big and Approve is never the default.
 * Closing puts the request off (the sheet's onClose defers it).
 */
export function ApprovalSheet({ approval }: { approval: Approval }) {
  const tr = useTr()
  const app = useApp()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const request = approval.request

  const decide = async (approve: boolean) => {
    setBusy(true)
    try {
      await app.decide(approval, approve)
      // The host's accessRequests event removes it; don't wait for that to close.
      app.deferApproval(approval)
    } catch (e) {
      setError(errorText(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <VStack className="settings-approval">
      <ModalHeader icon="lock.shield" title={tr(`Access request`)} />
      <VStack align="center" gap={16} className="settings-approval-body">
        <Icon name={deviceIcon(request.platform)} size={40} weight="light" color="var(--secondary)" />
        <VStack align="center" gap={4}>
          <Text as="div" size="title3" weight="semibold" className="settings-centered">
            {request.deviceName} wants to use {approval.store.hostName}
          </Text>
          {request.email ? <Text as="div" size="callout" tone="secondary">Signed in as {request.email}</Text> : null}
        </VStack>
        {request.code ? (
          <>
            <Text as="div" size={48} weight="semibold" mono tone="text" className="selectable settings-appear" aria-label={`Code ${request.code.split('').join(' ')}`}>
              {spacedCode(request.code)}
            </Text>
            <Text as="div" size="callout" tone="secondary" className="settings-centered">
              Approve only if {request.deviceName} shows exactly this code. If it doesn't, deny: someone may be trying to get in.
            </Text>
          </>
        ) : (
          <>
            <Spinner />
            <Text as="div" size="callout" tone="secondary">Waiting for {request.deviceName} to show its code…</Text>
          </>
        )}
        <Reveal show={error !== null}>
          <Text as="div" size="caption" tone="danger" className="settings-centered">{error}</Text>
        </Reveal>
        <HStack align="stretch" gap={12}>
          <Button kind="secondary" onClick={() => void decide(false)} disabled={busy}>
            Deny
          </Button>
          <Button onClick={() => void decide(true)} disabled={busy || !request.code}>
            Approve
          </Button>
        </HStack>
      </VStack>
    </VStack>
  )
}

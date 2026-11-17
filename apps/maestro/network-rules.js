// Helps `network-rules.yaml`, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed with `-e`).
// Gives the host's network a rule at `ask` and one at `auto`, the two levels between allow and deny; a host in no
// network gets a direct-only one of its own first (no relay to reach). `CLEANUP`, when the flow completes, leaves the
// network the script made, or puts back the rules the network had.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}
const any = (access) => ({ fromComputer: '*', fromAgent: '*', toAgent: '*', access })

if (typeof CLEANUP !== 'undefined') {
  const made = output.networkRules
  if (made && made.created) api('networkLeave', { network: made.network })
  else if (made) api('setNetworkRules', { network: made.network, rules: made.rules })
} else {
  let net = api('networkView', {}).networks[0]
  const created = !net
  if (created) {
    api('networkCreate', { network: 'maestro-rules', connector: 'private' })
    net = api('networkView', {}).networks[0]
  }
  output.networkRules = { network: net.id, created, rules: net.rules }
  api('setNetworkRules', { network: net.id, rules: [any('ask'), any('auto')] })
}

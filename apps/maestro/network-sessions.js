// Helps `network-sessions.yaml`, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed with `-e`).
// Registers a coding agent's session the way `codync-host mcp network` does on its first call (agentCall whoami);
// the host lists it on the computer's Network page for a week.
const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/agentCall`, {
  headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
  body: JSON.stringify({
    agent: { kind: 'codex', session: `maestro-${Date.now()}`, process: '1-maestro', cwd: '/work/api' },
    name: 'whoami',
    arguments: {},
  }),
})
if (res.status !== 200) throw new Error(`agentCall: ${res.status} ${res.body}`)
output.session = json(res.body).address

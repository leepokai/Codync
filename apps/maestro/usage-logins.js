// Helps `usage-logins.yaml`, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed with `-e`).
// LOGIN_PROFILE is a signed-in account login profile of Claude Code on that host. The script names it Work, so the
// usage rows list it, and posts made-up limits for it the way Claude Code's status line hook does. Phones in one run
// share the host: both steps can run twice. Without LOGIN_PROFILE the flow sits out.
if (typeof LOGIN_PROFILE !== 'undefined' && LOGIN_PROFILE) {
  const headers = { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' }
  const host = `http://127.0.0.1:${HOST_PORT}`
  const named = http.post(`${host}/api/renameLoginProfile`, {
    headers, body: JSON.stringify({ backend: 'claude', loginProfile: LOGIN_PROFILE, name: 'Work' }),
  })
  if (named.status !== 200) throw new Error(`renameLoginProfile: ${named.status} ${named.body}`)
  const resets_at = Math.floor(Date.now() / 1000) + 3600
  const limits = { five_hour: { used_percentage: 91, resets_at }, seven_day: { used_percentage: 40, resets_at } }
  const read = http.post(`${host}/ingest/statusline?loginProfile=${LOGIN_PROFILE}`, { headers, body: JSON.stringify({ rate_limits: limits }) })
  if (read.status !== 204) throw new Error(`statusline: ${read.status} ${read.body}`)
  output.login = 'Work'
}

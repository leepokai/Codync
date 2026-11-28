// Sets the computer's Delete old threads straight through the host's API (HOST_PORT and HOST_TOKEN, passed with `-e`),
// the way another device does, for `thread-retention.yaml`. `DAYS` is the choice to set (the first call keeps the
// host's own in `output.wasKeepDays`, which `RESTORE` puts back); `EXPECT` instead checks the days the host has now.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}

if (output.wasKeepDays === undefined) output.wasKeepDays = api('hello', {}).threadKeepDays || 0
if (typeof EXPECT !== 'undefined') {
  const days = api('hello', {}).threadKeepDays
  if (String(days) !== EXPECT) throw new Error(`the host keeps threads ${days} days, not ${EXPECT}`)
} else {
  api('setThreadKeepDays', { days: typeof RESTORE !== 'undefined' ? output.wasKeepDays : Number(DAYS) })
}

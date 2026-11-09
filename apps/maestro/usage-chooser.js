// Helps `usage-chooser.yaml`, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed with `-e`). Phones in
// one run share that host, so each takes agents of its own, by id, and asks only what its own actions did.
// Without `STEP` it picks them: one to switch, from the middle of the list (the ends are for dragging), starting off; and
// one to drag: the iPhone's goes from the top downward, Android's from the bottom upward, past the agent next to it.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}
const services = api('usage', {}).services
const ids = services.map((service) => service.id)
const onIphone = maestro.platform === 'ios'

if (typeof STEP === 'undefined') {
  const switched = services[Math.floor(services.length / 2) - (onIphone ? 1 : 0)]
  const dragged = onIphone ? 0 : services.length - 1
  api('setUsageShown', { id: switched.id, shown: false })
  output.agent = switched.name
  output.switched = switched.id
  output.dragged = services[dragged].name
  output.draggedId = ids[dragged]
  output.passed = ids[dragged + (onIphone ? 1 : -1)]
} else if (STEP === 'shown') {
  const shown = String(services.find((service) => service.id === output.switched).shown)
  if (shown !== EXPECT) throw new Error(`${output.switched} shown is ${shown}, expected ${EXPECT}`)
} else if (STEP === 'dragged') {
  const below = ids.indexOf(output.draggedId) > ids.indexOf(output.passed)
  if (below !== onIphone) throw new Error(`${output.draggedId} didn't pass ${output.passed}: ${ids}`)
}

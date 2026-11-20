export function composerEnter(event: {
  key: string; shiftKey: boolean; altKey: boolean; repeat: boolean
  isComposing: boolean; keyCode: number
}): 'send' | 'newline' | 'ignore' {
  if (event.key !== 'Enter' || event.isComposing || event.keyCode === 229) return 'ignore'
  if (event.shiftKey || event.altKey) return 'newline'
  return event.repeat ? 'ignore' : 'send'
}

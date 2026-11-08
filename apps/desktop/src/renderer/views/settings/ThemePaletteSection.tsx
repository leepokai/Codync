import { useEffect, useState } from 'react'
import { Button, CardSection, ChoicePicker } from '../../components/Controls'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useApp } from '../../store/context'

interface Themes {
  themes: string[]
  active: { name: string | null }
}

/**
 * Palettes from the person's own theme files (`~/.config/Codync/themes` and its kin), which some terminal
 * themes write. Files only: the app follows each change; the colors apply when one is chosen.
 */
export function ThemePaletteSection() {
  const tr = useTr()
  const app = useApp()
  const [themes, setThemes] = useState<string[]>([])
  const [name, setName] = useState('')
  useEffect(() => {
    let live = true
    const read = async () => {
      const local = app.local
      if (!local) return
      try {
        const answer = await (await local.ready()).call<Themes>('themeThemes')
        if (live) {
          setThemes(answer.themes)
          setName(answer.active.name ?? '')
        }
      } catch {
        // No host: leave what the screen has.
      }
    }
    void read()
    return () => { live = false }
  }, [app])
  const choose = async (next: string) => {
    const local = app.local
    if (!local) return
    setName(next)
    await (await local.ready()).call('setTheme', { name: next || null })
  }
  return (
    <CardSection title={tr('Theme palette')} footer={tr('Put JSON palettes in the theme folder and pick one here. The window follows each change while it is open.')}>
      {themes.length ? (
        <ChoicePicker
          selection={name}
          options={[{ id: '', label: tr('None') }, ...themes.map((theme) => ({ id: theme, label: theme }))]}
          onChange={(next) => void choose(next)}
        />
      ) : (
        <Text tone="secondary">{tr('No palette files yet.')}</Text>
      )}
      <Button kind="secondary" onClick={() => void choose('')} disabled={!name}>{tr('Use the app colors')}</Button>
    </CardSection>
  )
}

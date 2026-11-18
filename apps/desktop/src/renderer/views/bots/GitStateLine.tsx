import { gitState, type GitState } from '@shared/work'
import { IconButton } from '../../components/Controls'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { openLink } from '../../store/analytics'

/** The manual's page on what threads working at once do with and without git, and how to install it. */
const WORKING_FOLDERS = 'https://codync.dev/docs/working-folders'

const LINE: Record<GitState, string> = {
  repo: 'Git repository',
  none: 'Not a git repository',
  missing: "Git isn't installed on this computer",
}

/** Under a bot's folder in its settings: whether it is a git repository, with a button that opens the manual on it. */
export function GitStateLine({ bot }: { bot: { gitRepo: boolean; gitInstalled: boolean } }) {
  const tr = useTr()
  return (
    <HStack gap={4}>
      <Text size="caption" tone="secondary">{tr(LINE[gitState(bot)])}</Text>
      <IconButton title="About working folders and git" icon="info.circle" size={22} onClick={() => openLink(WORKING_FOLDERS, 'docs')} />
    </HStack>
  )
}

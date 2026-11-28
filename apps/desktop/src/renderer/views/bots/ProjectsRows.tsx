import { useState } from 'react'
import type { Bot, BotDraft } from '@shared/models'
import { DropdownMenu, IconButton, ValueRow } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader, Sheet } from '../../components/Overlay'
import { HStack, IconSlot, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useCopied } from '../../lib/use-copied'
import { useTr } from '../../lib/use-language'
import { lastPathComponent } from './drafts'
import { WORKTREE_MODES, sharedFolderName, withDefaultProject, withSharedFolder, withoutProject, withoutSharedFolder } from './projects'

/** The bot's own folder, where its chat, routines and files live: read-only, with a button that copies its path. */
export function HomeFolderRow({ bot }: { bot: Bot }) {
  const [copied, copy] = useCopied()
  return (
    <ValueRow label="Bot's folder" detail="Its chat, routines and files live here.">
      <Text truncate tone="secondary" title={bot.cwd}>{bot.cwd}</Text>
      <IconButton
        title={copied ? 'Path copied' : 'Copy path'}
        icon={copied ? 'checkmark' : 'square.on.square'}
        size={24}
        onClick={() => copy(bot.cwd)}
      />
    </ValueRow>
  )
}

/** The bot's projects, the default first: each with its full path, made the default or removed here; added with the
 * folder picker (`onAdd`). A git project's shared folders are edited in a sheet of their own. */
export function ProjectsRow({ draft, bot, sharesFolders, onChange, onAdd }: {
  draft: BotDraft
  /** The saved bot; null for a new one. */
  bot: Bot | null
  /** The host links shared folders into new worktrees (`hello.worktreeFill`). */
  sharesFolders: boolean
  onChange: (update: (d: BotDraft) => BotDraft) => void
  onAdd: () => void
}) {
  const tr = useTr()
  const [sharing, setSharing] = useState<string | null>(null)
  // Git flags come in the saved list's order: looked up by path, so a reorder or an unsaved add can't shift them.
  const isGit = (path: string) => bot?.projectGitRepos[bot.projects.indexOf(path)] === true
  return (
    <VStack gap={10}>
      <ValueRow label="Projects" detail="Threads start in the default project. The bot can work elsewhere when you ask.">
        <IconButton title="Add project" icon="plus" size={24} onClick={onAdd} />
      </ValueRow>
      {(draft.projects ?? []).map((path, at) => (
        <HStack key={path} gap={8} className="fade-in">
          <IconSlot width={18} tone="secondary"><Icon name={isGit(path) ? 'arrow.triangle.branch' : 'folder'} size={14} /></IconSlot>
          <VStack gap={2} align="flex-start" grow shrink>
            <Text truncate>{lastPathComponent(path)}</Text>
            <Text size="caption" tone="secondary" truncate title={path}>{path}</Text>
          </VStack>
          {at === 0
            ? <Text size="caption" tone="secondary">{tr('Default')}</Text>
            : <IconButton title="Make default" icon="star" size={24} onClick={() => onChange((d) => withDefaultProject(d, path))} />}
          {sharesFolders && isGit(path) ? <IconButton title="Shared folders" icon="link" size={24} onClick={() => setSharing(path)} /> : null}
          <IconButton title="Remove project" icon="minus.circle" size={24} onClick={() => onChange((d) => withoutProject(d, path))} />
        </HStack>
      ))}
      <Sheet open={sharing !== null} onClose={() => setSharing(null)} width={420}>
        {sharing ? <SharedFoldersSheet project={sharing} names={draft.sharedFolders?.[sharing] ?? []} onChange={onChange} /> : null}
      </Sheet>
    </VStack>
  )
}

/** One project's shared folders: each removed here, one added by typing its name. Saved with the bot. */
function SharedFoldersSheet({ project, names, onChange }: { project: string; names: string[]; onChange: (update: (d: BotDraft) => BotDraft) => void }) {
  const tr = useTr()
  const [typed, setTyped] = useState('')
  const name = sharedFolderName(typed)
  const add = () => {
    if (name === null) return
    onChange((d) => withSharedFolder(d, project, name))
    setTyped('')
  }
  return (
    <VStack>
      <ModalHeader icon="link" title={tr('Shared folders')} subtitle={lastPathComponent(project)} />
      <VStack gap={10} className="shared-folders-body">
        <Text size="callout" tone="secondary">{tr("Linked into each new worktree, so installed packages don't have to be installed again.")}</Text>
        {names.map((n) => (
          <HStack key={n} gap={8} className="fade-in">
            <IconSlot width={18} tone="secondary"><Icon name="folder" size={14} /></IconSlot>
            <Text truncate grow>{n}</Text>
            <IconButton title="Remove folder" icon="minus.circle" size={24} onClick={() => onChange((d) => withoutSharedFolder(d, project, n))} />
          </HStack>
        ))}
        <HStack gap={8}>
          <input className="field-box" autoFocus value={typed} placeholder="node_modules" aria-label={tr('Folder name')} spellCheck={false}
            onChange={(e) => setTyped(e.target.value)}
            onKeyDown={(e) => {
              if (e.key !== 'Enter' || e.nativeEvent.isComposing) return
              e.preventDefault()
              add()
            }} />
          <IconButton title="Add folder" icon="plus" size={24} disabled={name === null} onClick={add} />
        </HStack>
      </VStack>
    </VStack>
  )
}

/** When the bot's threads get a worktree of their own: for a bot with a project in a git repository. */
export function WorktreeModeRow({ draft, bot, onChange }: { draft: BotDraft; bot: Bot; onChange: (update: (d: BotDraft) => BotDraft) => void }) {
  const tr = useTr()
  const selection = draft.worktreeMode ?? bot.worktreeMode ?? 'sideBySide'
  const chosen = WORKTREE_MODES.find((m) => m.id === selection) ?? WORKTREE_MODES[1]!
  return (
    <ValueRow label="Worktree mode" detail={chosen.detail}>
      <DropdownMenu
        title={tr(chosen.label)}
        className="choice-picker outlined"
        items={() => WORKTREE_MODES.map((m) => ({
          title: m.label, detail: m.detail, selected: m.id === selection, action: () => onChange((d) => ({ ...d, worktreeMode: m.id })),
        }))}
      >
        <span className="choice-label">{tr(chosen.label)}</span>
        <Icon name="chevron.down" size={10} weight="semibold" color="var(--secondary)" />
      </DropdownMenu>
    </ValueRow>
  )
}

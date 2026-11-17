import type { Bot, BotDraft } from '@shared/models'

/** A full editor update must explicitly clear a previous model override. */
export function updatePayload(bot: Bot): BotDraft {
  return {
    id: bot.id, name: bot.name, description: bot.description, avatarColor: bot.avatarColor, avatarShape: bot.avatarShape,
    backend: bot.backend, command: bot.command ?? null, cwd: bot.managedWorkspace ? '' : bot.cwd, permission: bot.permission,
    model: bot.model ?? null, pinned: bot.pinned, hidden: bot.hidden, notify: bot.notify ?? null,
    connectors: bot.connectors, skills: bot.skills, computer: bot.computer, fastMode: bot.fastMode,
  }
}

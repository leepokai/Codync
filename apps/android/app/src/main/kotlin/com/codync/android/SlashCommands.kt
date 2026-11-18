package com.codync.android

import com.codync.android.core.Bot
import com.codync.android.core.FastModeState
import com.codync.android.core.GroupFastMode
import com.codync.android.core.InstalledSkill
import com.codync.android.core.fastModeDetail
import com.codync.android.core.fastModeState
import com.codync.android.core.runningAgent

/**
 * A command in the composer's `/` list (iOS `SlashCommand`): Fast mode, one of the bot's skills, or one of its agent's own
 * commands ([agent]: the agent that lists it, whose logo marks the row).
 */
internal data class SlashCommand(val name: String, val icon: String, val title: String, val detail: String, val warning: Boolean,
    val skill: String? = null, val agent: String? = null) {
    /** Picking it fills in `/<name> ` rather than running it. */
    val fills: Boolean get() = skill != null || agent != null
}

/**
 * The composer's `/` commands, listed while the draft is just `/partial`, the same in a group (iOS `SlashCommands`): Fast mode
 * runs here (a group's on each member; a new session is in the bot's menu); a skill fills in `/<id> `, which the host turns into
 * "read this skill first". A bot's own chat then lists its agent's commands (`agentCommands`), which fill in `/<name> ` and
 * reach the agent as typed.
 */
internal object SlashCommands {
    /** The command name being typed after a leading `/`, before any space. */
    fun query(draft: String): String? = if (draft.startsWith("/") && draft.none(Char::isWhitespace)) draft.drop(1) else null

    /**
     * Built-in commands, then the skills the bot has on (a group's: any member's), whose name or title holds `query`, then a bot's
     * agent's own commands whose name holds it. `members` are a group's bots.
     */
    fun list(bot: Bot, query: String, skills: List<InstalledSkill>, members: List<Bot> = emptyList()): List<SlashCommand> {
        val group = bot.kind == "group"
        val owners = if (group) members else listOf(bot)
        val own = skills.filter { skill -> owners.any { skill.id in it.skills } }.map { SlashCommand(it.id, "book.pages", it.name, it.description, false, it.id) }
        val codync = (builtIn(bot, members) + own).filter { query.isEmpty() || it.name.contains(query, true) || it.title.contains(query, true) }
        val agent = if (group) emptyList() else bot.agentCommands.map {
            SlashCommand(it.name, "", it.name, it.description.ifEmpty { it.hint.orEmpty() }, false, agent = bot.runningAgent)
        }
        return codync + agent.filter { query.isEmpty() || it.name.contains(query, true) }
    }

    /** The list in runs by where each command comes from: Codync (a null agent), or the agent that lists it (iOS `groups`). */
    fun groups(commands: List<SlashCommand>): List<Pair<String?, List<SlashCommand>>> =
        commands.fold(mutableListOf<Pair<String?, MutableList<SlashCommand>>>()) { groups, command ->
            if (groups.isNotEmpty() && groups.last().first == command.agent) groups.last().second += command
            else groups += command.agent to mutableListOf(command)
            groups
        }

    /** A draft that is just a built-in command (`/fast`): sending it runs the command. */
    fun builtIn(bot: Bot, draft: String, members: List<Bot> = emptyList()): SlashCommand? =
        query(draft.trim())?.let { name -> builtIn(bot, members).firstOrNull { it.name == name } }

    /** Fast mode, while the bot (or a member of the group) shows it. */
    private fun builtIn(bot: Bot, members: List<Bot>): List<SlashCommand> {
        val (detail, state) = if (bot.kind == "group") {
            val group = GroupFastMode(members)
            if (group.members.isEmpty()) return emptyList()
            group.detail to group.state
        } else {
            if (!bot.fastModeShown) return emptyList()
            bot.fastModeDetail to bot.fastModeState
        }
        return listOf(SlashCommand("fast", "bolt", "Fast mode", detail, state == FastModeState.Refused))
    }
}

/**
 * The `@partial` being typed at the end of a group's draft, if any. A member's full name and its space end it, so the list
 * doesn't stay up for a longer name that holds this one.
 */
internal fun mentionQuery(draft: String, members: List<Bot>): String? {
    val at = draft.lastIndexOf('@')
    if (at < 0 || at > 0 && !draft[at - 1].isWhitespace()) return null
    val query = draft.substring(at + 1)
    if (query.firstOrNull()?.isWhitespace() == true || query.contains('\n') || query.length > 24) return null
    return query.takeUnless { members.any { query.equals("${it.name} ", ignoreCase = true) } }
}

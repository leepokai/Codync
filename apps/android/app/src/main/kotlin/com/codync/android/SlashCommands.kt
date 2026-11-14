package com.codync.android

import com.codync.android.core.Bot
import com.codync.android.core.FastModeState
import com.codync.android.core.GroupFastMode
import com.codync.android.core.InstalledSkill
import com.codync.android.core.fastModeDetail
import com.codync.android.core.fastModeState

/** A command in the composer's `/` list (iOS `SlashCommand`): Fast mode, or one of the bot's skills. */
internal data class SlashCommand(val name: String, val icon: String, val title: String, val detail: String, val warning: Boolean,
    val skill: String? = null)

/**
 * The composer's `/` commands, listed while the draft is just `/partial`, the same in a group (iOS `SlashCommands`): Fast mode
 * runs here (a group's on each member; a new session is in the bot's menu); a skill fills in `/<id> `, which the host turns into
 * "read this skill first".
 */
internal object SlashCommands {
    /** The command name being typed after a leading `/`, before any space. */
    fun query(draft: String): String? = if (draft.startsWith("/") && draft.none(Char::isWhitespace)) draft.drop(1) else null

    /** Built-in commands, then the skills the bot has on (a group's: any member's), whose name or title holds `query`. `members` are a group's bots. */
    fun list(bot: Bot, query: String, skills: List<InstalledSkill>, members: List<Bot> = emptyList()): List<SlashCommand> {
        val owners = if (bot.kind == "group") members else listOf(bot)
        val own = skills.filter { skill -> owners.any { skill.id in it.skills } }.map { SlashCommand(it.id, "book.pages", it.name, it.description, false, it.id) }
        return (builtIn(bot, members) + own).filter { query.isEmpty() || it.name.contains(query, true) || it.title.contains(query, true) }
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

package com.codync.android

import android.content.Context
import androidx.core.content.edit
import com.codync.android.core.AppVersion
import com.codync.android.core.Bot
import java.text.Collator
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** One line of the roster as it shows: a bot, a section's header, or the "Bots" label (docs/plans/roster-sections.md). */
internal sealed interface RosterLine {
    val key: String

    data class Row(val bot: Bot) : RosterLine {
        override val key: String get() = bot.id
    }

    /** [count] and [unread] cover the section's unpinned bots, the ones shown under it. */
    data class Header(val name: String, val count: Int, val unread: Int, val folded: Boolean) : RosterLine {
        override val key: String get() = "section:$name"
    }

    /** The plain "Bots" label over the bots in no section. */
    data object Label : RosterLine {
        override val key: String get() = "label:bots"
    }
}

/** The lead and pinned bots stay on top, outside sections. */
internal val Bot.staysOnTop get() = lead || pinned

/**
 * The sorted [roster] as it shows: the lead and pinned bots, then each section by name (its header,
 * then its bots in roster order unless it's [folded]), then bots without a section, under
 * [RosterLine.Label] when there are sections. The lead and a pinned bot show only on top.
 */
internal fun rosterLines(roster: List<Bot>, folded: Set<String>): List<RosterLine> = buildList {
    val (pinned, unpinned) = roster.partition { it.staysOnTop }
    val sections = unpinned.groupBy(Bot::section)
    pinned.forEach { add(RosterLine.Row(it)) }
    val names = sectionNames(unpinned)
    for (name in names) {
        val bots = sections.getValue(name)
        val header = RosterLine.Header(name, bots.size, bots.sumOf { it.unread }, name in folded)
        add(header)
        if (!header.folded) bots.forEach { add(RosterLine.Row(it)) }
    }
    val loose = sections[null].orEmpty()
    if (names.isNotEmpty() && loose.isNotEmpty()) add(RosterLine.Label)
    loose.forEach { add(RosterLine.Row(it)) }
}

/** The sections [bots] are in, sorted the way the phone's language sorts ("İş" where Turkish puts it). */
internal fun sectionNames(bots: List<Bot>): List<String> = bots.mapNotNull(Bot::section).distinct().sortedWith(Collator.getInstance())

/** The sections a row's "Move to section" offers: hidden bots' too, though the roster leaves those out (iOS `BotStore.sections`). */
internal fun menuSections(bots: List<Bot>): List<String> = sectionNames(bots.filterNot(Bot::deleted))

/** The first host version that keeps sections (`section` on bots, `renameSection`). */
private const val SECTIONS_HOST = "2.32.0"

/** Section controls show only for a host that keeps sections; before its `hello` (offline) they stay hidden. */
internal val AppState.supportsSections: Boolean
    get() = hello?.text("version")?.let { !AppVersion.isBelow(it, SECTIONS_HOST) } == true

/** `updateBot`'s section patch: null goes as JSON null, which clears it (a missing key keeps it). */
internal fun sectionUpdate(id: String, section: String?): JsonObject = buildJsonObject { put("id", id); put("section", section) }

/** `renameSection`: every bot in [from] moves to [to], merging into a section of that name; null removes the section. */
internal fun sectionRename(from: String, to: String?): JsonObject = buildJsonObject { put("from", from); put("to", to) }

/** The most characters a section name may have (the host refuses more). */
private const val SECTION_NAME_LIMIT = 40

/** What a name field holds: at most [SECTION_NAME_LIMIT] characters (code points, as the host counts them), never cut inside a surrogate pair. */
internal fun limitSectionName(input: String): String =
    if (input.codePointCount(0, input.length) <= SECTION_NAME_LIMIT) input else input.substring(0, input.offsetByCodePoints(0, SECTION_NAME_LIMIT))

/** The section a typed [input] names: trimmed, or null when nothing but whitespace or control characters was typed (the host would clear or remove). */
internal fun sectionName(input: String): String? =
    input.trim().takeIf { name -> name.any { !it.isWhitespace() && !it.isISOControl() } }

/**
 * This device's folded sections after it renames section [from] to [to] (null removes it): a folded [from] stays folded
 * as [to], so a merge folds the section it joins; an unfolded one changes nothing.
 */
internal fun foldsAfterRename(folded: Set<String>, from: String, to: String?): Set<String> =
    if (from in folded) folded - from + listOfNotNull(to) else folded

/** This device's folded sections after the computer refused a rename that changed them from [before] to [set]: [before] again, unless they changed since. */
internal fun foldsAfterRefusal(current: Set<String>, set: Set<String>, before: Set<String>): Set<String> = if (current == set) before else current

/** A section change shown before the computer answered it (iOS `SectionMove`): bot [bot] into [to] (null: none). Told apart by identity. */
internal class SectionMove(val bot: String, val to: String?)

/** [bots] as they show while [pending] (oldest first) waits for the computer: each bot in the section its latest move put it in. */
internal fun withMoves(bots: List<Bot>, pending: List<SectionMove>): List<Bot> {
    if (pending.isEmpty()) return bots
    val sections = pending.associate { it.bot to it.to }
    return bots.map { bot -> if (bot.id in sections) bot.copy(section = sections[bot.id]) else bot }
}

/** The sections folded on this device (iOS `@AppStorage("collapsedRosterSections")`); not synced. */
internal object FoldedSections {
    private const val KEY = "collapsedRosterSections"
    private fun prefs(context: Context) = context.getSharedPreferences("codync-roster", Context.MODE_PRIVATE)
    fun load(context: Context): Set<String> = prefs(context).getStringSet(KEY, null).orEmpty().toSet()
    fun save(context: Context, names: Set<String>) { prefs(context).edit { putStringSet(KEY, names) } }
}

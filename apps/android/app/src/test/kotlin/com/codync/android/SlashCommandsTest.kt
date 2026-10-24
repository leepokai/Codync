package com.codync.android

import com.codync.android.core.Bot
import com.codync.android.core.FastModeState
import com.codync.android.core.GroupFastMode
import com.codync.android.core.InstalledSkill
import org.junit.Assert.*
import org.junit.Test

class SlashCommandsTest {
    @Test fun `only a leading slash word lists commands and the bot's skills`() {
        assertEquals("ne", SlashCommands.query("/ne"))
        assertNull(SlashCommands.query("/new session"))
        assertNull(SlashCommands.query("hi /new"))
        val bot = Bot("b", "Bot", skills = listOf("review"))
        val skills = listOf(InstalledSkill("review", "Review", "Reads diffs", "src", "/p"), InstalledSkill("other", "Other", "", "src", "/q"))
        assertEquals(listOf("review"), SlashCommands.list(bot, "", skills).map { it.name }.filter { it != "fast" })
        assertNull("a new session is in the bot's menu", SlashCommands.builtIn(bot, " /new "))
        assertTrue(SlashCommands.list(Bot("g", "Team", kind = "group"), "", skills).isEmpty())
    }

    @Test fun `a group lists its members' Fast mode and the union of their skills`() {
        val group = Bot("g", "Team", kind = "group", members = listOf("a", "b", "c"))
        val skills = listOf(InstalledSkill("review", "Review", "", "src", "/p"), InstalledSkill("notes", "Notes", "", "src", "/q"),
            InstalledSkill("unused", "Unused", "", "src", "/r"))
        val scout = Bot("a", "Scout", skills = listOf("review"))
        val writer = Bot("b", "Writer", skills = listOf("review", "notes"))
        val plain = Bot("c", "Plain", fastModeSupport = "unsupported")
        fun fast(vararg members: Bot) = SlashCommands.list(group, "fast", skills, members.toList()).single()

        assertEquals(listOf("fast", "review", "notes"), SlashCommands.list(group, "", skills, listOf(scout, writer, plain)).map { it.name })
        assertEquals("Turn on for every bot here: faster replies and more usage", fast(scout.copy(fastMode = true), writer).detail)
        // Members without Fast mode don't count toward "on".
        assertEquals("Turn off for every bot here and return to standard speed", fast(scout.copy(fastMode = true), writer.copy(fastMode = true), plain).detail)
        val refused = fast(scout.copy(fastMode = true), writer.copy(fastMode = true, fastModeSupport = "refused", fastModeIssue = "busy"))
        assertEquals("Writer: The agent turned it off: busy. Switch it off and on to try again.", refused.detail)
        assertTrue(refused.warning)
        assertEquals("fast", SlashCommands.builtIn(group, " /fast ", listOf(scout))?.name)
        assertEquals(listOf("review"), SlashCommands.list(group, "", skills, listOf(plain, Bot("d", "Old", fastModeSupport = "unsupported",
            skills = listOf("review")))).map { it.name })
        assertNull(SlashCommands.builtIn(group, "/fast", listOf(plain)))
    }

    @Test fun `a group's Fast mode turns every member that shows it the opposite way`() {
        val on = GroupFastMode(listOf(Bot("a", fastMode = true), Bot("b", fastMode = true), Bot("c", fastModeSupport = "unsupported")))
        assertEquals(listOf("a", "b"), on.members.map { it.id })
        assertEquals(FastModeState.On, on.state)
        assertEquals(FastModeState.Off, GroupFastMode(listOf(Bot("a", fastMode = true), Bot("b"))).state)
        assertEquals(FastModeState.Off, GroupFastMode(emptyList()).state)
    }

    @Test fun `mentions start after a space and stay short`() {
        assertEquals("Re", mentionQuery("hi @Re"))
        assertNull(mentionQuery("mail@host"))
        assertNull(mentionQuery("@" + "x".repeat(25)))
    }
}

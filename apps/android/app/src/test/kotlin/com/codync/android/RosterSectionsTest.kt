package com.codync.android

import com.codync.android.core.Bot
import com.codync.android.core.Computer
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.*
import org.junit.Test

class RosterSectionsTest {
    private val pinned = Bot("pin", pinned = true, section = "Work", unread = 7)
    private val work1 = Bot("w1", section = "Work", unread = 2)
    private val loose = Bot("loose")
    private val personal = Bot("p1", section = "Personal")
    private val work2 = Bot("w2", section = "Work", unread = 3)
    private val roster = listOf(pinned, work1, loose, personal, work2)

    @Test fun pinnedBotsComeFirstThenEachSectionByNameThenBotsWithoutOneUnderTheLabel() {
        assertEquals(listOf(
            RosterLine.Row(pinned),
            RosterLine.Header("Personal", count = 1, unread = 0, folded = false), RosterLine.Row(personal),
            RosterLine.Header("Work", count = 2, unread = 5, folded = false), RosterLine.Row(work1), RosterLine.Row(work2),
            RosterLine.Label, RosterLine.Row(loose),
        ), rosterLines(roster, folded = emptySet()))
    }

    @Test fun theLeadSitsInItsSectionLikeAnyBot() {
        val lead = Bot("lead", lead = true, section = "Work")
        assertEquals(listOf(
            RosterLine.Row(pinned),
            RosterLine.Header("Work", count = 2, unread = 2, folded = false), RosterLine.Row(work1), RosterLine.Row(lead),
            RosterLine.Label, RosterLine.Row(loose),
        ), rosterLines(listOf(pinned, work1, lead, loose), folded = emptySet()))
        assertEquals(listOf("lead", "w1"), moved(listOf(work1, lead), "lead", -1))
    }

    @Test fun aFoldedSectionKeepsItsHeaderAndUnreadAndHidesItsBots() {
        assertEquals(listOf(
            RosterLine.Row(pinned),
            RosterLine.Header("Personal", count = 1, unread = 0, folded = false), RosterLine.Row(personal),
            RosterLine.Header("Work", count = 2, unread = 5, folded = true),
            RosterLine.Label, RosterLine.Row(loose),
        ), rosterLines(roster, folded = setOf("Work")))
    }

    @Test fun sectionsSortTheWayTheLanguageDoesNotByCharacterCode() {
        val bots = listOf(Bot("a", section = "fig"), Bot("b", section = "Éclair"), Bot("c", section = "Cherry"), Bot("d", section = "banana"))
        val sorted = listOf("banana", "Cherry", "Éclair", "fig")
        assertEquals(sorted, sectionNames(bots))
        assertEquals(sorted, rosterLines(bots, emptySet()).filterIsInstance<RosterLine.Header>().map { it.name })
    }

    @Test fun namesAlikeButForCaseOrAccentsKeepOneOrderAsOnEveryClient() {
        for (names in listOf(listOf("Work", "work", "Café", "Cafe"), listOf("Cafe", "work", "Café", "Work"))) {
            assertEquals(listOf("Cafe", "Café", "work", "Work"), sectionNames(names.mapIndexed { i, name -> Bot("$i", section = name) }))
        }
    }

    @Test fun moveToSectionOffersAHiddenBotsSectionButNotADeletedOnes() {
        val bots = listOf(work1, Bot("archived", section = "Archive", hidden = true), Bot("gone", section = "Old", deleted = true))
        assertEquals(listOf("Archive", "Work"), menuSections(bots))
    }

    @Test fun withoutSectionsTheRosterShowsAsItIs() {
        val plain = listOf(Bot("pin", pinned = true), Bot("a"), Bot("b"))
        assertEquals(plain.map { RosterLine.Row(it) }, rosterLines(plain, emptySet()))
        // Only a pinned bot is in a section: no header, so no label.
        assertEquals(listOf(RosterLine.Row(pinned), RosterLine.Row(loose)), rosterLines(listOf(pinned, loose), emptySet()))
    }

    @Test fun theLabelNeedsABotInNoSection() {
        assertEquals(listOf(RosterLine.Header("Work", count = 1, unread = 2, folded = false), RosterLine.Row(work1)),
            rosterLines(listOf(work1), emptySet()))
    }

    @Test fun clearingASectionSendsNullRatherThanLeavingTheKeyOut() {
        assertEquals(buildJsonObject { put("id", "b"); put("section", "Work") }, sectionUpdate("b", "Work"))
        assertEquals(JsonNull, sectionUpdate("b", null)["section"])
        assertEquals(buildJsonObject { put("from", "Work"); put("to", "Jobs") }, sectionRename("Work", "Jobs"))
        assertEquals(JsonNull, sectionRename("Work", null)["to"])
    }

    @Test fun aTypedSectionNameIsTrimmedAndANameOfNothingIsNoName() {
        assertEquals("Work", sectionName("  Work \n"))
        assertEquals("Side projects", sectionName("Side projects"))
        listOf("", "   ", "\t\n", "\u00A0", "\u0000", " \u0000 \t").forEach { assertNull(it, sectionName(it)) }
    }

    @Test fun aNameFieldStopsAtFortyCharactersAsTheHostCountsThemNeverInsideAnEmoji() {
        val forty = "a".repeat(40)
        assertEquals(forty, limitSectionName(forty))
        assertEquals(forty, limitSectionName(forty + "b"))
        // An emoji is two UTF-16 units but one character: 40 of them fit, a 41st is cut whole.
        val emoji = "\uD83D\uDE00"
        assertEquals(emoji.repeat(40), limitSectionName(emoji.repeat(40)))
        assertEquals(emoji.repeat(40), limitSectionName(emoji.repeat(41)))
        assertEquals("a".repeat(39) + emoji, limitSectionName("a".repeat(39) + emoji + "b"))
    }

    @Test fun aFoldedSectionRenamedOrRemovedHereStaysFoldedUnderItsNewName() {
        assertEquals(setOf("Home", "Jobs"), foldsAfterRename(setOf("Home", "Work"), "Work", "Jobs"))
        assertEquals(setOf("Home"), foldsAfterRename(setOf("Home"), "Work", "Jobs")) // Unfolded: nothing changes.
        assertEquals(setOf("Home"), foldsAfterRename(setOf("Home"), "Work", "Home")) // Never unfolds the target.
        assertEquals(setOf("Home"), foldsAfterRename(setOf("Home", "Work"), "Work", null)) // Removed: its name goes.
        assertEquals(setOf("Home"), foldsAfterRename(setOf("Work"), "Work", "Home")) // A merge folds the section it joins.
        assertEquals(setOf("Work"), foldsAfterRename(setOf("Work"), "Work", "Work")) // The same name: still folded.
    }

    @Test fun aMoveShowsAtOnceAndARefusedOneLeavesTheBotWhereTheComputerOrALaterMovePutIt() {
        val first = SectionMove("w1", "Home")
        val second = SectionMove("w1", "Play")
        val out = SectionMove("p1", null)
        val section = { pending: List<SectionMove>, id: String -> withMoves(roster, pending).first { it.id == id }.section }
        assertEquals("Home", section(listOf(first), "w1"))
        assertEquals("Play", section(listOf(first, second), "w1")) // The latest move shows.
        assertEquals("Play", section(listOf(second), "w1")) // The first refused: the later one still shows.
        assertEquals("Work", section(emptyList(), "w1")) // Both refused: where the computer has it.
        assertNull(section(listOf(out), "p1"))
        assertEquals(roster, withMoves(roster, listOf(SectionMove("gone", "Home")))) // A bot not there: nothing changes.
    }

    @Test fun aRefusedRenamePutsTheFoldsBackUnlessOneChangedMeanwhile() {
        val before = setOf("Home", "Work")
        val moved = setOf("Home", "Jobs")
        assertEquals(before, foldsAfterRefusal(moved, moved, before))
        assertEquals(setOf("Jobs"), foldsAfterRefusal(setOf("Jobs"), moved, before)) // "Home" unfolded meanwhile: kept.
    }

    @Test fun moveToSectionShowsEachSectionByTheNameTheUserTypedNeverTranslated() {
        val translate = { title: String -> "translated $title" }
        val menu = sectionMenu(Bot("b", section = "Work"), listOf("Settings", "Work"), move = {}, newSection = {})
        assertEquals("translated Move to section", menu.shownTitle(translate))
        // A section named "Settings" stays "Settings"; the app's own rows translate.
        assertEquals(listOf("Settings", "translated New section…", "translated Remove from section"),
            menu.children.orEmpty().map { it.shownTitle(translate) })
    }

    @Test fun sectionControlsWaitForAHostThatKeepsSections() {
        fun host(version: String?) = AppState(Computer("fixture", "Studio", "fixture"), hello = version?.let { buildJsonObject { put("version", it) } })
        assertFalse(host("2.31.2").supportsSections)
        assertTrue(host("2.32.0").supportsSections)
        assertTrue(host("2.32.1-dev").supportsSections)
        assertFalse(host(null).supportsSections)
    }

    @Test fun leadChoiceWaitsForAHostThatLetsTheLeadChange() {
        fun host(version: String?) = AppState(Computer("fixture", "Studio", "fixture"), hello = version?.let { buildJsonObject { put("version", it) } })
        assertFalse(host("2.32.9").supportsLeadChoice)
        assertTrue(host("2.33.0").supportsLeadChoice)
        assertFalse(host(null).supportsLeadChoice)
    }
}

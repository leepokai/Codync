package com.codync.android.core

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class RoutineTest {
    @Test fun `WhatsApp reports require the sending account as well as the recipient`() {
        val output = RoutineOutput(kind = "whatsapp", target = "5215555555555@s.whatsapp.net")
        assertNotNull(RoutineOutputs.problem(output))
        assertNull(RoutineOutputs.problem(output.copy(account = "5215555555556@s.whatsapp.net")))
        assertTrue(RoutineOutputs.kinds.any { it.first == "whatsapp" })
    }
    @Test fun `a waiting retry reads as retrying, not queued`() {
        val run = RoutineRun("run", "routine", "bot", "pending")
        assertEquals("Queued", run.label)
        assertTrue(run.copy(notBefore = System.currentTimeMillis() + 60_000).label.startsWith("Retrying at "))
        assertEquals("Retrying", run.copy(notBefore = 1).label)
    }

    @Test fun `a run says what it stood for, skipped missed or waiting on the user`() {
        val run = RoutineRun("run", "routine", "bot", "running")
        assertEquals("Running", run.label)
        assertEquals("Waiting on you", run.copy(needsAttention = true).label)
        // A cancelled run that stands for a due occurrence is a skipped one, and says how many it dropped.
        assertEquals("Skipped", run.copy(status = "cancelled", scheduledFor = 1).label)
        assertEquals("Skipped · 2 missed", run.copy(status = "cancelled", scheduledFor = 1, missed = RoutineRunMissed(2, 2)).label)
        assertEquals("Skipped · 3 missed", run.copy(status = "cancelled", missed = RoutineRunMissed(3)).label)
        // A cancelled run with no occurrence just stopped.
        assertEquals("Cancelled", run.copy(status = "cancelled").label)
        // A failure that also dropped occurrences keeps both facts.
        assertEquals("Failed · 1 missed", run.copy(status = "failed", missed = RoutineRunMissed(1)).label)
    }

    @Test fun `editing unsupported schedules preserves every trigger field`() {
        val original = WireJson.parseToJsonElement("""[{"type":"event","source":"github","event":"*","filters":{"repo":"org/project"},"future":true},{"type":"interval","seconds":5400}]""").jsonArray.map { it.jsonObject }
        val schedule = RoutineScheduleDraft(kind = "once", original = original).forMobileEditor()
        val payload = RoutineDraft(name = "Updated name", instruction = "Keep my schedule", schedule = schedule).body("bot")
        assertEquals("keep", payload.getValue("schedule").jsonObject.getValue("kind").jsonPrimitive.content)
        assertEquals(JsonArray(original), payload.getValue("schedule").jsonObject.getValue("original"))
        assertFalse("enabled" in payload)
        assertFalse("id" in payload)
        assertEquals(JsonPrimitive("3600"), payload.getValue("timeoutSeconds"))
    }

    @Test fun `visual cron fields keep the style, zone and expression the host read`() {
        val draft = RoutineScheduleDraft(calendarStyle = "days", selectedDays = listOf(1, 3, 6), expression = "5 23 * * 1,3,6", zone = "Asia/Taipei")
        assertEquals(draft, draft.forMobileEditor())
    }

    @Test fun `kinds the editor cannot edit keep their original triggers`() {
        assertEquals("keep", RoutineScheduleDraft(kind = "once").forMobileEditor().kind)
        assertEquals("interval", RoutineScheduleDraft(kind = "interval").forMobileEditor().kind)
    }

    @Test fun `toggling a weekday keeps the chosen days sorted`() {
        val draft = RoutineScheduleDraft(calendarStyle = "days", selectedDays = listOf(1, 5))
        assertEquals(listOf(1, 3, 5), draft.toggleDay(3).selectedDays)
        assertEquals(listOf(5), draft.toggleDay(1).selectedDays)
    }

    @Test fun `the save body carries the policies only when set`() {
        val body = RoutineDraft(name = "n", instruction = "i", missedPolicy = "runOnce", limitPolicy = "skip").body("bot")
        assertEquals(JsonPrimitive("runOnce"), body.getValue("missedPolicy"))
        assertEquals(JsonPrimitive("skip"), body.getValue("limitPolicy"))
        // Absent policies keep the saved ones; the host defaults a new routine.
        val plain = RoutineDraft(name = "n", instruction = "i").body("bot")
        assertFalse("missedPolicy" in plain)
        assertFalse("limitPolicy" in plain)
    }

    @Test fun `a skipped run reads as the last run of its routine`() {
        val routine = Routine("r", "b", "Morning", "Report", emptyList(), true, triggerDescriptions = listOf("Every day"))
        assertEquals("Last run skipped", RoutineListing(listOf(routine), listOf(RoutineRun("s", "r", "b", "cancelled", scheduledFor = 1))).summary(routine))
        assertEquals("Last run skipped · 2 missed",
            RoutineListing(listOf(routine), listOf(RoutineRun("s", "r", "b", "cancelled", scheduledFor = 1, missed = RoutineRunMissed(2)))).summary(routine))
        // A cancelled run that only stopped says when it runs, not that it skipped.
        assertEquals("Every day", RoutineListing(listOf(routine), listOf(RoutineRun("c", "r", "b", "cancelled"))).summary(routine))
    }

    @Test fun `the yearly style asks for a time, on the month the host counts`() {
        assertTrue(RoutineSchedule.usesTime("yearly"))
        assertTrue("yearly" in RoutineSchedule.styles.map { it.first })
        assertEquals(29, RoutineSchedule.daysInMonth(2))
        assertEquals(31, RoutineSchedule.daysInMonth(7))
        assertEquals(30, RoutineSchedule.daysInMonth(9))
        assertEquals(listOf(1, 2, 3, 4, 6, 8, 12), RoutineSchedule.everyHours.map { it.first })
    }

    @Test fun `an event draft starts at a valid GitHub event and the editor can edit it`() {
        val draft = RoutineScheduleDraft()
        assertEquals("github", draft.eventSource)
        assertEquals("issues.opened", draft.eventName)
        assertTrue(draft.eventFilters.isEmpty())
        assertTrue("event" in RoutineSchedule.editableKinds)
        assertEquals("event", draft.copy(kind = "event").forMobileEditor().kind)
    }

    @Test fun `only calendar styles ask for a time`() {
        assertTrue(RoutineSchedule.usesTime("weekly"))
        assertFalse(RoutineSchedule.usesTime("hourly"))
        assertFalse(RoutineSchedule.usesTime("minutes"))
    }

    @Test fun `a routine's editor waits for the routine to be listed`() {
        val listing = RoutineListing(listOf(Routine("r", "bot", "Check CI", "Check", emptyList(), false)), emptyList())
        assertFalse(null.canEdit("r"))
        assertTrue(listing.canEdit("r"))
        assertFalse(listing.canEdit("other"))
        assertTrue(null.canEdit(""))
    }

    @Test fun `timeout reads as a human duration`() {
        for ((seconds, text) in listOf("3600" to "1 hour", "7200" to "2 hours", "1800" to "30 minutes", "60" to "1 minute",
            "90" to "90 seconds", "1" to "1 second", "0" to "0 seconds", "abc" to "abcs")) {
            assertEquals(text, RoutineDraft(timeoutSeconds = seconds).timeoutText)
        }
    }

    @Test fun `routine status reflects active recovery and failed runs before the schedule`() {
        val routine = Routine("r", "b", "Morning", "Report", emptyList(), true, triggerDescriptions = listOf("Every day"))
        val failed = RoutineRun("old", "r", "b", "failed")
        val active = RoutineRun("now", "r", "b", "recovering")
        assertEquals("Resuming after restart", RoutineListing(listOf(routine), listOf(failed, active)).summary(routine))
        assertEquals("Last run failed", RoutineListing(listOf(routine), listOf(failed)).summary(routine))
        assertEquals("Schedule needs attention", RoutineListing(emptyList(), emptyList()).summary(routine.copy(lastError = "invalid schedule")))
        assertEquals("Every day", RoutineListing(emptyList(), emptyList()).summary(routine))
    }
}

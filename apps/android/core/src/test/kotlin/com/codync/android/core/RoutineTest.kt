package com.codync.android.core

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class RoutineTest {
    @Test fun `a waiting retry reads as retrying, not queued`() {
        val run = RoutineRun("run", "routine", "bot", "pending")
        assertEquals("Queued", run.label)
        assertTrue(run.copy(notBefore = System.currentTimeMillis() + 60_000).label.startsWith("Retrying at "))
        assertEquals("Retrying", run.copy(notBefore = 1).label)
    }

    @Test fun `a listing carries the follow-ups still waiting, and an older host's has none`() {
        val listing = WireJson.decodeFromString<RoutineListing>("""{"routines":[],"runs":[],"followUps":[{"id":"f","botId":"b","at":1794650400000,"note":"Check the build","createdAt":1794649200000,"chain":0}]}""")
        assertEquals("Check the build", listing.followUps.single().note)
        assertTrue(WireJson.decodeFromString<RoutineListing>("""{"routines":[],"runs":[]}""").followUps.isEmpty())
    }

    @Test fun `a waiting follow-up says when it happens`() {
        val zone = java.time.ZoneId.of("Asia/Taipei")
        val now = java.time.ZonedDateTime.of(2026, 11, 14, 15, 0, 0, 0, zone).toInstant().toEpochMilli()
        val followUp = FollowUp("f", "b", now + 20 * 60_000, "Check the build")
        // The app language's way of writing a time, whatever the phone's is.
        val locale = java.util.Locale.forLanguageTag("zh-Hant")
        val time = { at: Long, format: java.time.format.DateTimeFormatter -> java.time.Instant.ofEpochMilli(at).atZone(zone).format(format.withLocale(locale)) }
        val short = java.time.format.DateTimeFormatter.ofLocalizedTime(java.time.format.FormatStyle.SHORT)
        assertEquals("Follow-up at " + time(followUp.at, short), followUp.line(locale, now, zone))
        val tomorrow = followUp.copy(at = now + 24 * 3_600_000)
        val dated = java.time.format.DateTimeFormatter.ofLocalizedDateTime(java.time.format.FormatStyle.MEDIUM, java.time.format.FormatStyle.SHORT)
        assertEquals("Follow-up at " + time(tomorrow.at, dated), tomorrow.line(locale, now, zone))
        // Due, and waiting its turn behind what the bot is doing.
        assertEquals("Follow-up due now", followUp.copy(at = now - 1).line(locale, now, zone))
    }

    @Test fun `editing unsupported schedules preserves every trigger field`() {
        val original = WireJson.parseToJsonElement("""[{"type":"event","source":"github","event":"*","filters":{"repo":"org/project"},"future":true},{"type":"interval","seconds":5400}]""").jsonArray.map { it.jsonObject }
        val schedule = RoutineScheduleDraft(kind = "interval", original = original).forMobileEditor()
        val payload = RoutineDraft(name = "Updated name", instruction = "Keep my schedule", schedule = schedule).body("bot")
        assertEquals("keep", payload.getValue("schedule").jsonObject.getValue("kind").jsonPrimitive.content)
        assertEquals(JsonArray(original), payload.getValue("schedule").jsonObject.getValue("original"))
        assertFalse("enabled" in payload)
        assertFalse("id" in payload)
        assertEquals(JsonPrimitive("3600"), payload.getValue("timeoutSeconds"))
    }

    @Test fun `mobile cron fields preserve the exact host expression and zone`() {
        val draft = RoutineScheduleDraft(calendarStyle = "days", expression = "5 23 * * 1,3,6", zone = "Asia/Taipei")
        assertEquals("custom", draft.forMobileEditor().calendarStyle)
        assertEquals(draft.expression, draft.forMobileEditor().expression)
        assertEquals(draft.zone, draft.forMobileEditor().zone)
    }

    @Test fun `the phone keeps its five repeat styles and opens the rest as custom`() {
        for ((style) in routineRepeats) assertEquals(style, RoutineScheduleDraft(calendarStyle = style).forMobileEditor().calendarStyle)
        for (style in listOf("minutes", "days", "monthly")) assertEquals("custom", RoutineScheduleDraft(calendarStyle = style).forMobileEditor().calendarStyle)
    }

    @Test fun `switching to custom starts from the cron the host compiled`() {
        val weekdays = RoutineScheduleDraft(calendarStyle = "weekdays", expression = "0 9 * * *")
        assertEquals("0 9 * * 1-5", weekdays.repeating("custom", "0 9 * * 1-5").expression)
        // While the host is still checking the last change, Custom starts empty, not from the cron the routine opened with.
        assertEquals("", weekdays.repeating("custom", null).expression)
        assertEquals("0 9 * * *", weekdays.repeating("hourly", "0 9 * * 1-5").expression)
        val custom = RoutineScheduleDraft(calendarStyle = "custom", expression = "7 9 * * 5")
        assertEquals(custom, custom.repeating("custom", null))
        val cron = WireJson.parseToJsonElement("""{"type":"cron","expression":"0 9 * * 1-5","timeZone":"Asia/Taipei"}""").jsonObject
        assertEquals("0 9 * * 1-5", RoutineSchedulePreview(weekdays, listOf(cron), "").cronExpression)
        assertNull(RoutineSchedulePreview(weekdays, listOf(buildJsonObject { put("type", "webhook") }), "").cronExpression)
    }

    @Test fun `a routine's editor waits for the routine to be listed`() {
        val listing = RoutineListing(listOf(Routine("r", "bot", "Check CI", "Check", emptyList(), false)), emptyList())
        assertFalse(null.canEdit("r"))
        assertTrue(listing.canEdit("r"))
        assertFalse(listing.canEdit("other"))
        assertTrue(null.canEdit(""))
    }

    @Test fun `timeout reads in the largest whole unit`() {
        for ((seconds, parts) in listOf("3600" to (1L to TimeoutUnit.Hour), "7200" to (2L to TimeoutUnit.Hour), "1800" to (30L to TimeoutUnit.Minute),
            "60" to (1L to TimeoutUnit.Minute), "90" to (90L to TimeoutUnit.Second), "1" to (1L to TimeoutUnit.Second),
            "0" to (0L to TimeoutUnit.Second), "abc" to null)) {
            assertEquals(parts, RoutineDraft(timeoutSeconds = seconds).timeoutParts)
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
        assertEquals("<Every day>", RoutineListing(emptyList(), emptyList()).summary(routine) { "<$it>" })
        assertEquals("<Last run <failed>>", RoutineListing(listOf(routine), listOf(failed)).summary(routine) { "<$it>" })
    }
}

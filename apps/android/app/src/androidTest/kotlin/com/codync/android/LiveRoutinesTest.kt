package com.codync.android

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import java.util.UUID

/** Opt-in isolated fake-agent fixture. All routines belong to a disposable clone. */
class LiveRoutinesTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()

    @Test fun routinesPreserveSchedulesAndPausedStateAndManageWebhookKeys() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("liveHost") == "true")
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val local = LocalStore(context)
        assertNull(PushPreferences.activeUser(context))
        val computer = local.computers().single()
        requireFixtureHost(computer.id)
        val route = computer.copy(urls = listOf("http://10.0.2.2:29322"), route = ConnectionRoute.DirectOnly)
        val channel = runBlocking { HostConnector.connect(route, local.identity()) }
        var clone: Bot? = null
        try {
            val bots = runBlocking { HostClient.bots(channel) }
            val parent = bots.single { it.name == "Android chat acceptance" }
            val response = runBlocking { channel.call("createBot", BotDraft.from(parent).copy(id = null, name = "Routine acceptance " + UUID.randomUUID().toString().take(6), permission = "auto").body()) }
            val bot = HostClient.decode(response.jsonObject.getValue("bot").jsonObject)
            clone = bot
            val store = ViewModelProvider(compose.activity)[AppStore::class.java]
            compose.waitUntil(30_000) { !store.state.value.loading && store.state.value.bots.any { it.id == bot.id } }
            compose.runOnUiThread { store.refreshUsage(false) }
            compose.waitUntil(15_000) { "usage" !in store.state.value.busy }
            assertNull(store.state.value.error)
            compose.runOnUiThread { store.openBot(bot.id) }
            compose.openChatDetails()
            compose.onNodeWithContentDescription("Ask the bot for a routine").performScrollTo().performClick()
            compose.waitUntil(10_000) { compose.onAllNodes(hasSetTextAction() and hasText("I want a routine that ")).fetchSemanticsNodes().size == 1 }
            compose.runOnUiThread { store.editDraft(bot.id, "") }
            compose.openChatDetails()
            compose.waitUntil(10_000) { "routines:${bot.id}" !in store.state.value.busy && store.state.value.routines[bot.id] != null }
            compose.onNodeWithText("No routines yet. Ask the bot for one, or set it up yourself with +.").assertIsDisplayed()
            compose.onNodeWithContentDescription("Set up a routine").performScrollTo().performClick()
            field(Input.Name).performTextReplacement("Schedule acceptance")
            field(Input.Instruction).performTextReplacement("Report a fake-agent acceptance result")
            field(Input.Cron).performTextReplacement("invalid cron")
            compose.waitUntil(10_000) { compose.onAllNodesWithText("Check schedule again").fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithText("Create routine").assertIsNotEnabled()
            field(Input.Cron).performTextReplacement("0 0 1 1 *")
            field(Input.Zone).performTextReplacement("America/New_York")
            waitEnabled("Create routine")
            compose.onNodeWithText("Create routine").performClick()
            compose.waitUntil(15_000) { store.state.value.routines[bot.id]?.routines?.size == 1 && "routines:${bot.id}" !in store.state.value.busy }
            val schedule = store.state.value.routines.getValue(bot.id).routines.single()
            val originalNext = schedule.nextRunAt
            // Each row's switch pauses and resumes it.
            toggle("Schedule acceptance").assertIsOn().performClick()
            compose.waitUntil(15_000) { store.state.value.routines.getValue(bot.id).routines.single().enabled == false && "routines:${bot.id}" !in store.state.value.busy }
            compose.onNodeWithText("Schedule acceptance").performScrollTo().performClick()
            compose.onNodeWithText("Edit routine").assertIsDisplayed()
            field(Input.Name).performTextReplacement("Edited schedule")
            waitEnabled("Save changes")
            compose.onNodeWithText("Save changes").performClick()
            try { compose.waitUntil(15_000) { store.state.value.routines.getValue(bot.id).routines.single().name == "Edited schedule" && "routines:${bot.id}" !in store.state.value.busy } }
            catch (_: Throwable) { throw AssertionError("Routine save failed: ${store.state.value.error}; names=${store.state.value.routines[bot.id]?.routines?.map { it.name }}; busy=${store.state.value.busy}") }
            val edited = store.state.value.routines.getValue(bot.id).routines.single()
            assertFalse(edited.enabled)
            assertEquals(schedule.triggers, edited.triggers)
            toggle("Edited schedule").assertIsOff().performClick()
            compose.waitUntil(15_000) { store.state.value.routines.getValue(bot.id).routines.single().enabled && "routines:${bot.id}" !in store.state.value.busy }
            assertEquals(originalNext, store.state.value.routines.getValue(bot.id).routines.single().nextRunAt)

            // Unsupported multiple triggers and filters survive an unrelated edit, even paused.
            val triggers = WireJson.parseToJsonElement("""[{"type":"event","source":"github","event":"pull_request.opened","filters":{"repo":"org/project"}},{"type":"interval","seconds":31536000}]""").jsonArray
            val original = runBlocking { channel.call("saveRoutine", buildJsonObject {
                put("botId", bot.id); put("name", "Preserve triggers"); put("instruction", "Keep the original filters")
                put("triggers", triggers); put("enabled", false)
            }) }.jsonObject.getValue("routine").jsonObject
            compose.waitUntil(15_000) { store.state.value.routines.getValue(bot.id).routines.size == 2 && "routines:${bot.id}" !in store.state.value.busy }
            compose.onNodeWithText("Preserve triggers").performScrollTo().performClick()
            field(Input.Name).performTextReplacement("Preserved triggers")
            waitEnabled("Save changes")
            compose.onNodeWithText("Save changes").performClick()
            try { compose.waitUntil(15_000) { store.state.value.routines.getValue(bot.id).routines.any { it.name == "Preserved triggers" } && "routines:${bot.id}" !in store.state.value.busy } }
            catch (_: Throwable) { throw AssertionError("Trigger-preserving save failed: ${store.state.value.error}; names=${store.state.value.routines[bot.id]?.routines?.map { it.name }}") }
            val preserved = store.state.value.routines.getValue(bot.id).routines.single { it.name == "Preserved triggers" }
            assertEquals(original.getValue("triggers"), JsonArray(preserved.triggers))
            assertFalse(preserved.enabled)

            // A new webhook routine stays open on its URL and key.
            compose.onNodeWithContentDescription("Set up a routine").performScrollTo().performClick()
            field(Input.Name).performTextReplacement("Webhook acceptance")
            field(Input.Instruction).performTextReplacement("Report a fake webhook acceptance result")
            compose.onNodeWithContentDescription("When to run: Schedule").performScrollTo().performClick()
            compose.onNodeWithText("Webhook").performClick()
            waitEnabled("Create routine")
            compose.onNodeWithText("Create routine").performClick()
            compose.waitUntil(15_000) { compose.onAllNodesWithContentDescription("Replace key").fetchSemanticsNodes().isNotEmpty() }
            val webhookRoutine = store.state.value.routines.getValue(bot.id).routines.single { it.name == "Webhook acceptance" }
            val firstKey = runBlocking { channel.call("routineWebhook", body("botId" to bot.id, "id" to webhookRoutine.id)) }.jsonObject.getValue("key")
            compose.onNodeWithContentDescription("Replace key").performScrollTo().performClick()
            compose.onNodeWithText("Replace the key?").assertIsDisplayed()
            compose.onNodeWithText("Replace key").performClick()
            compose.waitUntil(15_000) { compose.onAllNodesWithContentDescription("Hide key").fetchSemanticsNodes().isNotEmpty() }
            val nextKey = runBlocking { channel.call("routineWebhook", body("botId" to bot.id, "id" to webhookRoutine.id)) }.jsonObject.getValue("key")
            assertNotEquals(firstKey, nextKey)
            compose.onNodeWithContentDescription("Test run").performClick()
            compose.waitUntil(15_000) { compose.onAllNodesWithContentDescription("Started").fetchSemanticsNodes().isNotEmpty() }
            compose.waitUntil(30_000) { store.state.value.routines.getValue(bot.id).runs.any { it.routineId == webhookRoutine.id && it.finishedAt != null } }
            val run = store.state.value.routines.getValue(bot.id).runs.first { it.routineId == webhookRoutine.id }
            assertEquals("succeeded", run.status)
            compose.onNodeWithContentDescription("Delete routine").performClick()
            compose.onNodeWithText("Delete routine?").assertIsDisplayed()
            compose.onNodeWithText("Delete routine").performClick()
            compose.waitUntil(15_000) { store.state.value.routines.getValue(bot.id).routines.none { it.id == webhookRoutine.id } }
            compose.onNodeWithText("Webhook acceptance").assertDoesNotExist()
        } finally {
            clone?.let { bot -> runBlocking { channel.call("deleteBot", body("botId" to bot.id)) } }
            channel.close()
        }
    }

    private enum class Input { Name, Instruction, Cron, Zone }

    /** The open editor's text fields, top to bottom: name, instruction, then a schedule's cron and time zone. */
    private fun field(field: Input): SemanticsNodeInteraction {
        val inEditor = hasSetTextAction() and hasAnyAncestor(hasScrollAction() and hasAnyDescendant(hasText("When to run")))
        compose.waitUntil(10_000) { compose.onAllNodes(inEditor and isEnabled()).fetchSemanticsNodes().size > field.ordinal }
        return compose.onAllNodes(inEditor)[field.ordinal].performScrollTo()
    }
    private fun toggle(routine: String) = compose.onNode(hasContentDescription(routine) and isToggleable()).performScrollTo()
    private fun waitEnabled(label: String) {
        compose.waitUntil(10_000) { compose.onAllNodes(hasText(label) and isEnabled()).fetchSemanticsNodes().size == 1 }
    }
}

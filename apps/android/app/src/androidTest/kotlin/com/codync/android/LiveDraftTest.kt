package com.codync.android

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.core.content.FileProvider
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import java.io.File
import kotlinx.serialization.json.*

/** The controller kills the emulator process between prepare and restore. Isolated host only. */
class LiveDraftTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()

    @Test fun draftsSurviveProcessDeathAndQueueUsingTheirPersistedNonce() {
        val phase = InstrumentationRegistry.getArguments().getString("draftPhase")
        assumeTrue(phase in listOf("prepare", "restore"))
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val local = LocalStore(context)
        val computer = local.computers().single()
        requireFixtureHost(computer.id)
        assertNull(PushPreferences.activeUser(context))
        val book = DraftBook(local.directory, computer.id)
        val metadata = File(context.cacheDir, "draft-acceptance.json")
        val store = ViewModelProvider(compose.activity)[AppStore::class.java]
        compose.waitUntil(30_000) { store.state.value.bots.any { it.name == "Android chat acceptance" } }
        val bot = store.state.value.bots.single { it.name == "Android chat acceptance" }
        compose.runOnUiThread { store.openBot(bot.id) }
        // A cached roster can arrive before startup finishes; wait for the chat itself.
        compose.waitUntil(10_000) { !store.state.value.loading && store.state.value.rosterLoaded &&
            compose.onAllNodes(chatTitle).fetchSemanticsNodes().size == 1 }
        val mainText = "Android unsent draft acceptance"
        if (phase == "prepare") {
            compose.waitUntil(10_000) { compose.onAllNodes(hasSetTextAction()).fetchSemanticsNodes().size == 1 }
            compose.onNode(hasSetTextAction()).performTextReplacement(mainText)
            compose.waitUntil(10_000) { store.state.value.drafts[bot.id]?.text == mainText }
            val source = File(context.cacheDir, "shared-attachments/draft-acceptance/draft.txt").apply {
                requireNotNull(parentFile).mkdirs(); writeText("Private draft attachment acceptance")
            }
            val uri = FileProvider.getUriForFile(context, context.packageName + ".files", source)
            compose.runOnUiThread { store.importFiles(listOf(uri)) { store.addDraftFiles(bot.id, it) } }
            try { compose.waitUntil(10_000) { store.state.value.drafts[bot.id]?.files?.size == 1 } }
            catch (_: Throwable) { throw AssertionError("Draft import failed: ${store.state.value.error}; busy=${store.state.value.busy}") }
            try { compose.waitUntil(10_000) { book.read().any { it.botId == bot.id && it.files.size == 1 && it.text == mainText } } }
            catch (_: Throwable) {
                val current = store.state.value.drafts[bot.id]
                throw AssertionError("Draft persistence failed: mainText=${current?.text == mainText}, files=${current?.files?.size}, error=${store.state.value.error}")
            }
            val main = book.read().single { it.botId == bot.id }
            metadata.writeText(buildJsonObject { put("nonce", main.nonce) }.toString())
        } else {
            val saved = Json.parseToJsonElement(metadata.readText()).jsonObject
            val nonce = saved.getValue("nonce").jsonPrimitive.content
            val main = store.state.value.drafts.getValue(bot.id)
            assertEquals(nonce, main.nonce)
            assertEquals("Private draft attachment acceptance", File(main.files.single().path).readText())
            compose.onNode(hasSetTextAction() and hasText(mainText)).assertIsDisplayed()
            compose.onNodeWithContentDescription("Remove draft.txt").assertIsDisplayed()
            compose.onNodeWithContentDescription("Send").performClick()
            compose.waitUntil(30_000) { store.state.value.entries.any { it.botId == bot.id && it.clientNonce == nonce } }
            val sent = store.state.value.entries.single { it.botId == bot.id && it.clientNonce == nonce }
            assertFalse(store.state.value.drafts.containsKey(bot.id))
            compose.waitUntil(10_000) { book.read().none { it.nonce == nonce } }
            compose.waitUntil(30_000) { store.state.value.entries.any { it.botId == bot.id && it.kind == "permission" && it.status == "pending" } }
            compose.onNodeWithText("Allow once").performClick()
            compose.waitUntil(30_000) { store.state.value.entries.any { it.botId == bot.id && it.seq > sent.seq && it.kind == "agent" && it.isChat } }
            compose.waitUntil(10_000) { book.read().none { it.botId == bot.id } }
            metadata.delete()
        }
    }
}

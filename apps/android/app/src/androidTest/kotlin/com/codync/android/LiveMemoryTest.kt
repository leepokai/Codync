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

/** The caller seeds and deletes a disposable bot with one memory of its own on the isolated fake-agent host. */
class LiveMemoryTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()

    @Test fun forgettingAnEntryAndClearingMemoryPreserveTheConversation() {
        val botId = InstrumentationRegistry.getArguments().getString("memoryBot")
        assumeTrue(botId != null)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val local = LocalStore(context)
        val computer = local.computers().single().copy(urls = listOf("http://10.0.2.2:29322"), route = ConnectionRoute.DirectOnly)
        val channel = runBlocking { HostConnector.connect(computer, local.identity()) }
        try {
            val before = runBlocking { channel.call("history", body("botId" to botId)) }
            val store = ViewModelProvider(compose.activity)[AppStore::class.java]
            compose.waitUntil(30_000) { store.state.value.bots.any { it.id == botId } }
            val name = store.state.value.bots.single { it.id == botId }.name
            assertTrue(name.startsWith("Android memory acceptance"))
            // The memory card is in the whole settings form, the roster's Edit profile.
            compose.runOnUiThread { store.openBot(null) }
            compose.rosterAction(name, "Edit profile")
            val entry = "Android memory profile acceptance"
            compose.waitUntil(30_000) { compose.onAllNodesWithText(entry).fetchSemanticsNodes().size == 1 }
            compose.onNodeWithText("Android memory history acceptance").performScrollTo().assertIsDisplayed()
            // Searching leaves one fact, so its menu is the only one.
            val search = hasSetTextAction() and hasText("Search memories")
            compose.onNode(search).performScrollTo().performTextInput("profile")
            compose.waitUntil(30_000) { compose.onAllNodesWithContentDescription("Memory actions").fetchSemanticsNodes().size == 1 }
            compose.onNodeWithContentDescription("Memory actions").performScrollTo().performClick()
            compose.onNodeWithText("Forget").performClick()
            compose.waitUntil(30_000) { compose.onAllNodesWithText(entry).fetchSemanticsNodes().isEmpty() }
            val left = WireJson.decodeFromJsonElement<BotMemory>(runBlocking { channel.call("memory", body("botId" to botId)) })
            assertTrue(left.bot.entries.none { it.text == entry })
            compose.onNodeWithContentDescription("More").performScrollTo().performClick()
            compose.onNodeWithText("Forget everything here").performClick()
            compose.onNodeWithText("Forget everything this bot remembers?").assertIsDisplayed()
            compose.onNodeWithText("Forget everything").performClick()
            val after = runBlocking { channel.call("history", body("botId" to botId)) }
            assertEquals(before.jsonObject.getValue("entries"), after.jsonObject.getValue("entries"))
            val memory = WireJson.decodeFromJsonElement<BotMemory>(runBlocking { channel.call("memory", body("botId" to botId)) })
            assertTrue(memory.bot.entries.isEmpty())
        } finally { channel.close() }
    }
}

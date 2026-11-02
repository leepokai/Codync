package com.codync.android

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test

/** The caller seeds and deletes a disposable bot on the isolated fake-agent host. */
class LiveMemoryTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()

    @Test fun theMemoryTabShowsBothLayersAndDreamsReadOnly() {
        val botId = InstrumentationRegistry.getArguments().getString("memoryBot")
        assumeTrue(botId != null)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val local = LocalStore(context)
        val computer = local.computers().first().copy(urls = listOf("http://10.0.2.2:29322"), route = ConnectionRoute.DirectOnly)
        val channel = runBlocking { HostConnector.connect(computer, local.identity()) }
        try {
            val memory = WireJson.decodeFromJsonElement<BotMemory>(runBlocking { channel.call("memory", body("botId" to botId)) })
            val store = compose.computerStore()
            compose.waitUntil(30_000) { store.state.value.bots.any { it.id == botId } }
            val name = store.state.value.bots.single { it.id == botId }.name
            compose.runOnUiThread { store.openBot(botId) }
            // The chat's title opens its details; Memory is one of their pages.
            compose.onNodeWithContentDescription("$name, view details").performClick()
            compose.onNodeWithText("Memory").performClick()
            compose.waitUntil(30_000) { compose.onAllNodesWithText("What this bot learned about you", substring = true).fetchSemanticsNodes().isNotEmpty() }
            for (entry in memory.bot.entries) compose.onNodeWithText(entry.text, substring = true).performScrollTo().assertIsDisplayed()
            compose.onNodeWithText("About you").performClick()
            compose.waitUntil(30_000) { compose.onAllNodesWithText("Use what your bots share").fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithText("Dreams").performClick()
            compose.waitUntil(30_000) { compose.onAllNodesWithText("Between chats the bot tidies its memory from your own words.").fetchSemanticsNodes().isNotEmpty() }
            // Read-only: nothing here adds, edits or forgets a memory.
            assertTrue(compose.onAllNodesWithContentDescription("Add memory").fetchSemanticsNodes().isEmpty())
        } finally { channel.close() }
    }
}

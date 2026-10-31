package com.codync.android

import android.graphics.Bitmap
import android.os.Build
import androidx.activity.compose.setContent
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import java.io.File

class ChatLayoutTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()

    private fun exchange(id: String, seq: Long, chat: String, source: String, target: String, text: String, reply: String? = null,
        status: String = "completed", detail: String? = null) =
        Entry(id, seq, chat, rev = seq, kind = "notice", createdAt = System.currentTimeMillis() - 60_000 + seq, data = buildJsonObject {
            put("text", text); put("status", status)
            put("botMessage", buildJsonObject {
                put("sourceBotId", source); put("targetBotId", target); put("text", text)
                reply?.let { put("reply", it) }; detail?.let { put("detail", it) }
            })
        })

    @Test fun aBotToBotMessageIsOneLineThatOpensTheirConversation() {
        assumeTrue(Build.PRODUCT.startsWith("sdk"))
        val store = previewStore()
        val dex = Bot("exchange-preview", "Dex"); val miles = Bot("miles-preview", "Miles")
        val request = "Investigate the connection error."
        val answer = "The caller cancelled while audio was connecting."
        val state = AppState(previewComputer, loading = false, selectedBot = dex.id, bots = listOf(dex, miles),
            connection = LinkState.Ready(HostRoute.Direct), actualConnection = LinkState.Ready(HostRoute.Direct),
            historyComplete = setOf(dex.id), entries = listOf(exchange("bot-message", 1, dex.id, miles.id, dex.id, request, answer)))
        compose.runOnUiThread { compose.activity.setContent { Hosted { ChatScreen(state, store, {}) } } }
        // The chat keeps one line (iOS `BotMessageRow`), not the messages themselves.
        compose.onNodeWithContentDescription("Message from Miles").assertIsDisplayed()
        compose.onNodeWithText(request).assertDoesNotExist()
        if (InstrumentationRegistry.getArguments().getString("layoutSnapshots") == "true") {
            val bitmap = compose.onRoot().captureToImage().asAndroidBitmap()
            File(compose.activity.cacheDir, "bot-message-preview.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        }
        compose.onNodeWithContentDescription("Message from Miles").performClick()
        // The conversation, oldest first: Miles's request, then Dex's reply.
        compose.waitUntil(5_000) { compose.onAllNodesWithText(answer).fetchSemanticsNodes().isNotEmpty() }
        compose.waitForIdle() // the sheet has finished sliding up
        compose.onNodeWithContentDescription("Dex and Miles").assertIsDisplayed()
        val asked = compose.onNodeWithText(request).assertIsDisplayed().fetchSemanticsNode().boundsInRoot
        val replied = compose.onNodeWithText(answer).assertIsDisplayed().fetchSemanticsNode().boundsInRoot
        assertTrue(asked.bottom <= replied.top)
    }

    @Test fun consecutiveExchangesWithOneBotAreOneLineThatSaysWhenOneFailed() {
        assumeTrue(Build.PRODUCT.startsWith("sdk"))
        val store = previewStore()
        val dex = Bot("exchange-run-preview", "Dex"); val miles = Bot("miles-run-preview", "Miles")
        val state = AppState(previewComputer, loading = false, selectedBot = dex.id, bots = listOf(dex, miles),
            connection = LinkState.Ready(HostRoute.Direct), actualConnection = LinkState.Ready(HostRoute.Direct),
            historyComplete = setOf(dex.id), entries = listOf(
                exchange("asked", 1, dex.id, dex.id, miles.id, "Review the fix.", "Looks good."),
                exchange("failed", 2, dex.id, dex.id, miles.id, "Ship it.", status = "failed", detail = "Recipient stopped.")))
        compose.runOnUiThread { compose.activity.setContent { Hosted { ChatScreen(state, store, {}) } } }
        compose.onNodeWithContentDescription("2 messages with Miles, failed").assertIsDisplayed()
        compose.onNodeWithText("Review the fix.").assertDoesNotExist()
        compose.onNodeWithText("Recipient stopped.").assertDoesNotExist()
    }

    @Test fun aShortConversationStartsAtTheTopAndRetainsContextActions() {
        assumeTrue(Build.PRODUCT.startsWith("sdk"))
        val store = previewStore()
        val bot = Bot("layout-preview", "Scout", avatarColor = "green", avatarShape = "cloud")
        val sent = "Add a weekly totals helper and check its tests."
        val reply = "I added the helper and a test. All four tests pass."
        val now = System.currentTimeMillis()
        val state = AppState(previewComputer, loading = false, selectedBot = bot.id, bots = listOf(bot),
            connection = LinkState.Ready(HostRoute.Direct), actualConnection = LinkState.Ready(HostRoute.Direct),
            historyComplete = setOf(bot.id), entries = listOf(
                Entry("user-layout", 1, bot.id, rev = 1, kind = "user", data = buildJsonObject { put("text", sent) }, createdAt = now),
                Entry("agent-layout", 2, bot.id, rev = 1, kind = "agent", data = buildJsonObject { put("text", reply); put("author", bot.id); put("final", true) }, createdAt = now)))
        compose.runOnUiThread { compose.activity.setContent { Hosted { ChatScreen(state, store, {}) } } }
        compose.onNodeWithText("You").assertDoesNotExist()
        compose.onNodeWithText("Message actions").assertDoesNotExist()
        compose.onNodeWithText("Reply").assertDoesNotExist()
        compose.onAllNodesWithText(bot.name).assertCountEquals(1)
        val user = compose.onNode(hasText(sent) and hasClickAction())
        val agent = compose.onNode(hasText(reply) and hasClickAction())
        val userBounds = user.fetchSemanticsNode().boundsInRoot
        val agentBounds = agent.fetchSemanticsNode().boundsInRoot
        val root = compose.onRoot().fetchSemanticsNode().boundsInRoot
        assertTrue(userBounds.left > agentBounds.left)
        assertTrue(userBounds.right > agentBounds.right)
        // Like the iPhone's list, a conversation shorter than the screen starts at the top.
        assertTrue(userBounds.top < root.height / 2)
        compose.onNodeWithContentDescription("Add files").assertIsDisplayed()
        compose.onNodeWithContentDescription("Call").assertIsDisplayed()
        if (InstrumentationRegistry.getArguments().getString("layoutSnapshots") == "true") {
            val bitmap = compose.onRoot().captureToImage().asAndroidBitmap()
            File(compose.activity.cacheDir, "layout-chat-preview.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        }
        // Held in its middle: the rounded corners are outside the bubble.
        user.performTouchInput { longClick() }
        compose.onNodeWithText("Reply").assertIsDisplayed()
        compose.onNodeWithContentDescription("👍").assertIsDisplayed()
        compose.onNodeWithText("Copy").assertIsDisplayed()
        compose.onNodeWithText("Show what it did").assertDoesNotExist()
        InstrumentationRegistry.getInstrumentation().sendKeyDownUpSync(android.view.KeyEvent.KEYCODE_BACK)
        compose.onNodeWithText("Copy").assertDoesNotExist()
    }

    @Test fun theTitleOpensTheBotsDetailsOnDetails() {
        assumeTrue(Build.PRODUCT.startsWith("sdk"))
        val store = previewStore()
        val bot = Bot("details-preview", "Scout")
        val state = AppState(previewComputer, loading = false, selectedBot = bot.id, bots = listOf(bot), historyComplete = setOf(bot.id),
            hello = buildJsonObject { put("version", ARTIFACTS_HOST) })
        compose.runOnUiThread { compose.activity.setContent { Hosted { ChatScreen(state, store, {}) } } }
        compose.onNode(chatTitle).performClick()
        // Details first, with the routines; then Library, Memory and Settings, as on the iPhone.
        compose.onNode(hasText("Details") and isSelected()).assertIsDisplayed()
        compose.onNodeWithText("Library").assertIsDisplayed()
        compose.onNodeWithText("Memory").assertIsDisplayed()
        // The sheet merges plain text into itself, so the heading is found unmerged.
        compose.onNodeWithText("Routines", useUnmergedTree = true).assertExists()
        compose.onNodeWithText("Settings").performClick()
        compose.onNodeWithText("Notifications").performScrollTo().assertIsDisplayed()
    }
}

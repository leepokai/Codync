package com.codync.android

import android.content.ClipboardManager
import android.os.Build
import android.view.KeyEvent
import androidx.compose.foundation.layout.*
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.UriHandler
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.dp
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import com.codync.android.design.MenuItem
import com.codync.android.design.ReactionPick
import com.codync.android.design.contextActions
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test

/** A message's long-press menu (`Modifier.contextActions`): reactions over copy, reply and trace. */
class MessageActionsMenuTest {
    @get:Rule val compose = createComposeRule()
    private val store = previewStore()
    private fun back() = InstrumentationRegistry.getInstrumentation().sendKeyDownUpSync(KeyEvent.KEYCODE_BACK)

    @Before fun isolatedEmulatorOnly() { assumeTrue(Build.PRODUCT.startsWith("sdk")) }

    @Test fun holdingAReplyOffersReactionsAndCopiesItsText() {
        val bot = Bot("message-preview", "Scout")
        val text = "Checked the connection. The saved route works."
        val entry = Entry("reply", 1, bot.id, rev = 1, kind = "agent", data = buildJsonObject { put("text", text); put("final", true) })
        val initial = AppState(previewComputer, loading = false, selectedBot = bot.id, bots = listOf(bot),
            connection = LinkState.Ready(HostRoute.Direct), historyComplete = setOf(bot.id), entries = listOf(entry))
        val state = mutableStateOf(initial)
        compose.setContent { Hosted { ChatScreen(state.value, store, {}) } }
        val bubble = compose.onNode(hasText(text, substring = true) and hasClickAction())
        bubble.performTouchInput { longClick() }
        compose.onNodeWithContentDescription("👍").assertIsDisplayed().assertIsNotSelected()
        compose.onNodeWithText("Reply").assertIsDisplayed()
        compose.onNodeWithText("Show what it did").assertIsDisplayed()
        compose.onNodeWithText("Copy").performClick()
        compose.onNodeWithText("Copy").assertDoesNotExist()
        val clipboard = InstrumentationRegistry.getInstrumentation().targetContext.getSystemService(ClipboardManager::class.java)
        compose.runOnIdle { assertEquals(text, clipboard.primaryClip?.getItemAt(0)?.text?.toString()) }
        bubble.performTouchInput { longClick() }
        back()
        compose.onNodeWithText("Copy").assertDoesNotExist()
        // A retired account/session cannot keep the old message's actions alive.
        bubble.performTouchInput { longClick() }
        compose.onNodeWithText("Copy").assertIsDisplayed()
        compose.runOnUiThread { state.value = initial.copy(generation = initial.generation + 1) }
        compose.onNodeWithText("Copy").assertDoesNotExist()
        compose.runOnUiThread { clipboard.clearPrimaryClip() }
    }

    @Test fun replyQuotesTheMessageInTheComposerUntilCancelled() {
        val bot = Bot("reply-preview", "Scout")
        val text = "The saved route works."
        val reply = Entry("reply", 1, bot.id, rev = 1, kind = "agent", data = buildJsonObject { put("text", text); put("final", true) })
        val quoted = Entry("quoted", 2, bot.id, rev = 2, kind = "user", data = buildJsonObject {
            put("text", "Ship it"); put("replyTo", buildJsonObject { put("id", reply.id); put("author", bot.id); put("text", text) })
        })
        val state = AppState(previewComputer, loading = false, selectedBot = bot.id, bots = listOf(bot), connection = LinkState.Ready(HostRoute.Direct),
            historyComplete = setOf(bot.id), entries = listOf(reply, quoted))
        var shown by mutableStateOf(state)
        compose.setContent { Hosted { ChatScreen(shown, store, {}) } }
        // The sent reply shows what it quotes.
        compose.onNodeWithText("“$text”").assertIsDisplayed()
        compose.onNode(hasText(text) and hasClickAction()).performTouchInput { longClick() }
        compose.onNodeWithText("Reply").performClick()
        compose.onAllNodesWithText("“$text”").assertCountEquals(2)
        compose.onNodeWithText("Reply…").assertIsDisplayed()
        compose.onNodeWithContentDescription("Cancel reply").performClick()
        compose.onAllNodesWithText("“$text”").assertCountEquals(1)
        compose.onNodeWithContentDescription("Cancel reply").assertDoesNotExist()
        // An older host can't quote: no Reply.
        compose.runOnUiThread { shown = state.copy(hello = buildJsonObject { put("version", "2.20.0") }) }
        compose.onNode(hasText(text) and hasClickAction()).performTouchInput { longClick() }
        compose.onNodeWithText("Copy").assertIsDisplayed()
        compose.onNodeWithText("Reply").assertDoesNotExist()
        back()
    }

    @Test fun reactionsAndOptionsCloseTheMenuAndRunOnce() {
        val invoked = mutableListOf<String>()
        compose.setContent { Hosted {
            Box(Modifier.fillMaxSize().safeDrawingPadding().padding(top = 80.dp)) {
                Text("Action callback preview", Modifier.contextActions(ReactionPick(listOf("👍", "❤️"), listOf("👍")) { invoked.add(it) },
                    preview = { Text("Action callback preview") }) {
                    listOf(MenuItem("Reply") { invoked.add("reply") }, MenuItem("Copy") { invoked.add("copy") })
                })
            }
        } }
        val bubble = compose.onNode(hasText("Action callback preview") and hasClickAction())
        bubble.performTouchInput { longClick() }
        compose.onNodeWithContentDescription("👍").assertIsSelected().performClick()
        compose.onNodeWithText("Copy").assertDoesNotExist()
        assertEquals(listOf("👍"), invoked)
        for ((label, event) in listOf("Reply" to "reply", "Copy" to "copy")) {
            bubble.performTouchInput { longClick() }
            compose.onNodeWithText(label).performClick()
            compose.onNodeWithText(label).assertDoesNotExist()
            assertEquals(event, invoked.last())
        }
        bubble.performClick()
        compose.onNodeWithText("Copy").assertDoesNotExist()
        bubble.performTouchInput { longClick() }
        back()
        compose.onNodeWithText("Copy").assertDoesNotExist()
        assertEquals(3, invoked.size)
    }

    @Test fun linksKeepTheirTapAndHoldingOpensTheMenu() {
        val urls = mutableListOf<String>()
        compose.setContent { Hosted {
            CompositionLocalProvider(LocalUriHandler provides object : UriHandler {
                override fun openUri(uri: String) { urls.add(uri) }
            }) {
                Box(Modifier.safeDrawingPadding().contextActions { listOf(MenuItem("Copy")) }) {
                    MarkdownText("[Visit Codync](https://codync.dev)", selectable = false)
                }
            }
        } }
        val link = compose.onNodeWithText("Visit Codync", useUnmergedTree = true)
        link.performTouchInput { click() }
        compose.onNodeWithText("Copy").assertDoesNotExist()
        assertEquals(listOf("https://codync.dev"), urls)
        link.performTouchInput { longClick() }
        compose.onNodeWithText("Copy").assertIsDisplayed()
        assertEquals(1, urls.size)
    }
}

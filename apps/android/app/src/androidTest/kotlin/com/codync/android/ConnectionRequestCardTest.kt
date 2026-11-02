package com.codync.android

import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import com.codync.android.core.*
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

/** A bot's connection request in the chat. Read-only: no test submits a credential to the store. */
class ConnectionRequestCardTest {
    @get:Rule val compose = createComposeRule()
    private val bot = Bot("bot", "Scout")
    private val store = previewStore()
    private val online = AppState(previewComputer, loading = false, bots = listOf(bot), actualConnection = LinkState.Ready(HostRoute.Direct))

    private fun card(request: ConnectionRequest, text: String = "") = Entry("request", 1, bot.id, rev = 1, kind = "connectionRequest",
        data = buildJsonObject { put("text", text); put("connectionRequest", WireJson.encodeToJsonElement(ConnectionRequest.serializer(), request)) })

    @Test fun aLoginNeedsAMaskedPasswordAndTheComputerBeforeSaving() {
        val state = mutableStateOf(online)
        // A site without a dot: no favicon to fetch.
        val entry = card(ConnectionRequest("login", "pending", "Fixture login", bot.id, site = "fixture"))
        compose.setContent { Hosted { ConnectionRequestCard(entry, state.value, store, { _, _ -> fail("Not an app") }, { _, _ -> fail("Not a connector") }) } }
        compose.onNodeWithText("Save").assertIsNotEnabled()
        compose.onNodeWithContentDescription("Username or email").performTextInput("fixture-user")
        compose.onNodeWithContentDescription("Password or op:// reference")
            .assert(SemanticsMatcher.keyIsDefined(SemanticsProperties.Password)).performTextInput("fixture-password")
        compose.onNodeWithText("Save").assertIsEnabled()
        compose.onNodeWithText("Saved securely on this computer. The bot types it into the sign-in page but never sees it.").assertIsDisplayed()
        compose.runOnUiThread { state.value = online.copy(actualConnection = LinkState.Connecting) }
        compose.onNodeWithText("Save").assertIsNotEnabled()
    }

    @Test fun anAppRequestOpensItsSheetForThisRequest() {
        val opened = mutableListOf<Pair<String, ComposioApp>>()
        val entry = card(ConnectionRequest("app", "pending", "GitHub", bot.id, toolkit = "github"), "Needs GitHub to open the PR.")
        compose.setContent { Hosted { ConnectionRequestCard(entry, online, store, { id, app -> opened.add(id to app) }, { _, _ -> fail("Not a connector") }) } }
        compose.onNodeWithText("Needs GitHub to open the PR.").assertIsDisplayed()
        compose.onNodeWithText("Connect").performClick()
        compose.waitUntil { opened.isNotEmpty() }
        assertEquals("request", opened.single().first)
        assertEquals("github", opened.single().second.slug)
    }

    @Test fun finishedAndUnknownRequestsNeverLookPending() {
        val request = mutableStateOf(ConnectionRequest("connection", "ready", "Fixture connector", bot.id, connectorId = "fixture"))
        compose.setContent { Hosted { ConnectionRequestCard(card(request.value), online, store, { _, _ -> }, { _, _ -> }) } }
        compose.onNodeWithText("Connected").assertIsDisplayed()
        compose.onNodeWithText("Connect").assertDoesNotExist()
        compose.runOnUiThread { request.value = request.value.copy(status = "future-status") }
        compose.onNodeWithText("Cancelled").assertIsDisplayed()
        compose.onNodeWithText("Connected").assertDoesNotExist()
        compose.onNodeWithText("Connect").assertDoesNotExist()
    }

    @Test fun aRequestForAnUnknownBotAsksForANewerApp() {
        val entry = card(ConnectionRequest("secret", "pending", "Fixture key", "elsewhere"))
        compose.setContent { Hosted { ConnectionRequestCard(entry, online, store, { _, _ -> }, { _, _ -> }) } }
        compose.onNodeWithText("This connection request needs a newer app.").assertIsDisplayed()
        compose.onNodeWithText("Connect").assertDoesNotExist()
    }
}

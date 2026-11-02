package com.codync.android

import android.view.View
import android.view.ViewGroup
import android.webkit.WebView
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import kotlin.coroutines.resume

/** Opt-in inert ACP provider, real host PTY. Never starts a real provider's login. */
class LiveTerminalTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    @Test fun agentKeysStayOnTheHostAndCanBePreservedOrRemoved() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("liveSetupKeys") == "true")
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        assertNull(PushPreferences.activeUser(context))
        val local = LocalStore(context)
        val computer = local.computers().single()
        requireFixtureHost(computer.id)
        runBlocking {
            val channel = HostConnector.connect(computer.copy(urls = listOf("http://10.0.2.2:29322"), route = ConnectionRoute.DirectOnly), local.identity())
            val backend = "android-setup-fixture"
            fun keys(values: Map<String, String>) = buildJsonObject { put("backend", backend); put("vars", WireJson.encodeToJsonElement(values)) }
            try {
                val before = channel.call("agentAuth", body("backend" to backend))
                val initial = WireJson.decodeFromJsonElement<AgentAuth>(before)
                assertEquals(false, initial.signedIn)
                assertTrue(initial.savedEnv.isEmpty())
                assertTrue(initial.methods.any { it.kind == "envVar" && it.vars.any { field -> field.name == "CODYNC_ACCEPTANCE_KEY" } })
                val saved = channel.call("setAgentEnv", keys(mapOf("CODYNC_ACCEPTANCE_KEY" to "fixture-key")))
                val status = WireJson.decodeFromJsonElement<AgentAuth>(saved)
                assertEquals(true, status.signedIn)
                assertEquals(listOf("CODYNC_ACCEPTANCE_KEY"), status.savedEnv)
                assertFalse(saved.toString().contains("\"fixture-key\""))
                assertFalse(status.detail.orEmpty().contains("fixture-key"))
                val preserved = channel.call("setAgentEnv", keys(emptyMap()))
                assertEquals(true, WireJson.decodeFromJsonElement<AgentAuth>(preserved).signedIn)
                val removed = channel.call("setAgentEnv", keys(mapOf("CODYNC_ACCEPTANCE_KEY" to "")))
                val final = WireJson.decodeFromJsonElement<AgentAuth>(removed)
                assertEquals(false, final.signedIn)
                assertTrue(final.savedEnv.isEmpty())
            } finally {
                try { channel.call("setAgentEnv", keys(mapOf("CODYNC_ACCEPTANCE_KEY" to "")), 2_000) }
                finally { channel.close() }
            }
        }
    }
    @Test fun aRealSetupTerminalClosesOnBackAndSurvivesRotationBeforeSignIn() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("liveSetup") == "true")
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        assertNull(PushPreferences.activeUser(context))
        val local = LocalStore(context); val computer = local.computers().single()
        requireFixtureHost(computer.id)
        val channel = runBlocking { HostConnector.connect(computer.copy(urls = listOf("http://10.0.2.2:29322"), route = ConnectionRoute.DirectOnly), local.identity()) }
        val setup = buildJsonObject { put("backend", "android-setup-fixture"); put("step", "login"); put("method", "fixture-terminal"); put("cols", 80); put("rows", 24) }
        var term: String? = null
        fun find(view: View): WebView? {
            if (view is WebView) return view
            if (view is ViewGroup) for (i in 0 until view.childCount) { val found = find(view.getChildAt(i)); if (found != null) return found }
            return null
        }
        fun terminalText(): String = runBlocking { withContext(Dispatchers.Main) {
            val view = find(compose.activity.window.decorView) ?: return@withContext ""
            withTimeout(5_000) { suspendCancellableCoroutine { reply -> view.evaluateJavascript("typeof codyncTerminal === 'undefined' ? '' : codyncTerminal.text()") {
                if (reply.isActive) reply.resume(Json.parseToJsonElement(it).jsonPrimitive.content)
            } } }
        } }
        try {
            val store = compose.computerStore()
            compose.waitUntil(30_000) { !store.state.value.loading && store.state.value.actualConnection is LinkState.Ready }
            compose.runOnUiThread { store.openBot(null); store.refreshBackends() }
            compose.waitUntil(15_000) { store.state.value.hello?.get("backends")?.jsonArray?.any { it.jsonObject["id"]?.jsonPrimitive?.content == "android-setup-fixture" } == true }
            compose.openExtensions()
            compose.onNode(hasSetTextAction() and hasText("Search agents, connectors and skills")).performTextInput("Android setup fixture")
            compose.onNode(hasSetTextAction() and hasText("Android setup fixture")).performImeAction()
            compose.onNode(hasText("Android setup fixture") and hasClickAction() and !hasSetTextAction()).performScrollTo().performClick()
            compose.waitUntil(20_000) { compose.onAllNodesWithText("Fixture terminal sign-in").fetchSemanticsNodes().size == 1 }
            compose.onNodeWithText("Fixture terminal sign-in").performScrollTo().performClick()
            compose.waitUntil(20_000) { terminalText().contains("Fixture setup ready: café 🐈") }
            val first = runBlocking { channel.call("agentSetup", setup) }
            term = first.jsonObject.getValue("term").jsonPrimitive.content
            compose.onNodeWithContentDescription("Back").assertIsDisplayed().performClick()
            compose.waitUntil(5_000) { compose.onAllNodesWithContentDescription("Check again").fetchSemanticsNodes().size == 1 }
            runBlocking { withTimeout(10_000) {
                while (true) {
                    try { channel.call("termInput", body("term" to term, "data" to "")); delay(100) }
                    catch (error: HostError) { assertTrue(error.message.contains("gone")); break }
                }
            } }
            compose.waitUntil(20_000) { compose.onAllNodesWithText("Fixture terminal sign-in").fetchSemanticsNodes().size == 1 }
            compose.onNodeWithText("Fixture terminal sign-in").performScrollTo().performClick()
            compose.waitUntil(20_000) { terminalText().contains("Fixture setup ready: café 🐈") }
            val second = runBlocking { channel.call("agentSetup", setup) }
            term = second.jsonObject.getValue("term").jsonPrimitive.content
            assertNotEquals(first.jsonObject.getValue("term").jsonPrimitive.content, term)
            compose.activityRule.scenario.recreate()
            compose.waitUntil(20_000) { terminalText().contains("Fixture setup ready: café 🐈") }
            val afterRotation = runBlocking { channel.call("agentSetup", setup) }
            assertEquals(term, afterRotation.jsonObject.getValue("term").jsonPrimitive.content)
            compose.onNodeWithText("Keyboard").performClick()
            instrumentation.waitForIdleSync()
            android.os.SystemClock.sleep(500) // Wait for native IME attach before requesting its connection.
            val committed = CompletableDeferred<Boolean>()
            compose.runOnUiThread {
                val view = requireNotNull(find(compose.activity.window.decorView))
                val connection = requireNotNull(view.onCreateInputConnection(android.view.inputmethod.EditorInfo()))
                val handler = connection.handler ?: android.os.Handler(android.os.Looper.getMainLooper())
                handler.post { committed.complete(connection.commitText("fixture-answer", 1)) }
            }
            val delivered = runBlocking { withTimeout(5_000) { committed.await() } }
            assertTrue(delivered)
            compose.waitUntil(5_000) { terminalText().contains("fixture-answer") }
            instrumentation.sendKeyDownUpSync(android.view.KeyEvent.KEYCODE_ENTER)
            compose.waitUntil(15_000) { compose.onAllNodesWithText("Finished.").fetchSemanticsNodes().size == 1 }
            assertTrue(terminalText().contains("Fixture answer accepted"))
            compose.onNodeWithText("Done").performClick()
            compose.waitUntil(20_000) { compose.onAllNodesWithText("Signed in.").fetchSemanticsNodes().size == 1 }
        } finally {
            try { term?.let { id -> runBlocking { channel.call("termClose", body("term" to id), 2_000) } } }
            finally { channel.close() }
        }
    }
}

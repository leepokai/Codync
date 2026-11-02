package com.codync.android

import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.*
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import com.codync.android.core.*
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import java.util.concurrent.atomic.AtomicBoolean

class AgentSettingsScreenTest {
    @get:Rule val compose = createComposeRule()
    private val online = AppState(previewComputer, loading = false,
        actualConnection = LinkState.Ready(HostRoute.Direct))

    @Composable private fun Picker(state: AppState, backend: String, selected: String?, choose: (String?) -> Unit, load: suspend (String) -> AgentModels) =
        AgentModelPicker(state, load, backend, selected, emptyMap(), false, null, { id, _ -> choose(id) }, {})

    private fun showing(text: String) = compose.onAllNodesWithText(text).fetchSemanticsNodes().isNotEmpty()

    @Test fun hostModelsRetainAnUnknownOverrideAndFoldTheRecommendedDefault() {
        var selected by mutableStateOf<String?>("old-model")
        compose.setContent { Hosted { Picker(online, "agent", selected, { selected = it }) {
            AgentModels(listOf(AgentModel("auto", "Default (recommended)"), AgentModel("host-model", "Host model", "From this agent")))
        } } }
        // The picker is one summary row ("agent · model") that unfolds into the model list.
        compose.onNodeWithText("agent · old-model").assertIsDisplayed().performClick()
        compose.waitUntil { showing("Host model") }
        compose.onAllNodesWithText("Default (recommended)").assertCountEquals(1)
        compose.onNodeWithText("Default (recommended)").performClick()
        assertNull(selected)
        compose.onNodeWithText("Default").performClick()
        compose.onNodeWithContentDescription("Refresh model list").assertIsDisplayed()
        compose.onNodeWithText("Host model").performClick()
        assertEquals("host-model", selected)
    }

    @Test fun aLateCancelledModelDiscoveryCannotReplaceAnotherAgentsCatalog() {
        var backend by mutableStateOf("old")
        val started = AtomicBoolean(false); val returned = AtomicBoolean(false)
        compose.setContent { Hosted { Column {
            TextButton(onClick = { backend = "new" }) { Text("Switch agent") }
            Picker(online, backend, null, {}) { id ->
                if (id == "old") withContext(NonCancellable) { started.set(true); delay(600); returned.set(true) }
                AgentModels(listOf(AgentModel(id, "$id advertised model")))
            }
        } } }
        compose.waitUntil { started.get() }
        compose.onNodeWithText("Switch agent").performClick()
        compose.onNodeWithText("new · Default").performClick()
        compose.waitUntil { showing("new advertised model") }
        compose.waitUntil(3_000) { returned.get() }
        compose.waitForIdle()
        compose.onNodeWithText("new advertised model").assertIsDisplayed()
        compose.onNodeWithText("old advertised model").assertDoesNotExist()
    }

    @Test fun retryModelListAlsoFillsThePermissionsModes() {
        val failing = AtomicBoolean(true)
        val load: suspend (String) -> AgentModels = {
            if (failing.get()) throw java.io.IOException("Agent not ready")
            AgentModels(emptyList(), modes = listOf(AgentMode("auto", "Auto"), AgentMode("plan", "Plan")))
        }
        compose.setContent { Hosted { Column {
            var reload by remember { mutableIntStateOf(0) }
            AgentModelPicker(online, load, "agent", null, emptyMap(), false, null, { _, _ -> }, {}, onReload = { reload++ })
            ApprovalRow(online, load, BotDraft(backend = "agent", mode = "plan"), {}, reload)
        } } }
        compose.onNodeWithText("agent · Default").performClick()
        compose.waitUntil { compose.onAllNodesWithContentDescription("Retry model list").fetchSemanticsNodes().isNotEmpty() }
        // Until the list loads, Permissions shows the bot's mode by its id.
        compose.onNodeWithText("Plan").assertDoesNotExist()
        failing.set(false)
        compose.onNodeWithContentDescription("Retry model list").performClick()
        compose.waitUntil { showing("Plan") }
    }

    @Test fun anOfflineModelOverrideRemainsEditableWithoutInventingAProviderList() {
        var selected by mutableStateOf<String?>("my-model")
        compose.setContent { Hosted { Picker(online.copy(actualConnection = LinkState.Connecting), "agent", selected, { selected = it }) {
            fail("Offline must not query models"); AgentModels(emptyList())
        } } }
        compose.onNodeWithText("agent · my-model").performClick()
        compose.onNodeWithText("Connect to this computer to load its models. You can still enter a model ID.").assertIsDisplayed()
        compose.onNodeWithContentDescription("Retry model list").assertIsDisplayed()
        compose.onNode(hasSetTextAction() and hasText("my-model")).performTextReplacement("saved-model")
        assertEquals("saved-model", selected)
    }

    @Test fun hostFolderBrowsingFiltersLocallyDrillsInAndSelectsTheAuthoritativeDirectory() {
        var chosen: String? = null
        val requests = mutableListOf<String?>()
        compose.setContent { Hosted { FolderPicker("/home", true, "Fixture computer", load = { path ->
            requests.add(path)
            if (path == "/home/Repo") DirListing("/home/Repo", "/home", true, emptyList(), recent = emptyList())
            else DirListing("/home", "/", false, listOf(DirItem("Repo", "/home/Repo", true), DirItem("Other", "/home/Other")), recent = emptyList())
        }, select = { it }) { chosen = it } } }
        compose.waitUntil { compose.onAllNodesWithText("Browse other folders").fetchSemanticsNodes().size == 1 }
        compose.onNodeWithText("Search recent workspaces").assertDoesNotExist()
        compose.onNodeWithText("Browse other folders").performClick()
        compose.waitUntil { compose.onAllNodesWithText("Repo").fetchSemanticsNodes().size == 1 }
        compose.onNode(hasSetTextAction()).performTextInput("repo")
        compose.onNodeWithText("Other").assertDoesNotExist()
        assertEquals(listOf(null, "/home"), requests)
        compose.onNodeWithText("Repo").performClick()
        compose.waitUntil { requests.lastOrNull() == "/home/Repo" }
        compose.onNodeWithText("Use this folder").performClick()
        compose.waitUntil { chosen == "/home/Repo" }
        assertEquals(listOf(null, "/home", "/home/Repo"), requests)
        compose.onNodeWithContentDescription("Parent folder").performClick()
        compose.waitUntil { compose.onAllNodesWithText("Other").fetchSemanticsNodes().size == 1 }
        compose.onNodeWithText("Use this folder").performClick()
        compose.waitUntil { chosen == "/home" }
    }

    @Test fun anUnavailableOrOfflineFolderShowsWhyWithoutOfferingIt() {
        val online = mutableStateOf(true)
        var loads = 0
        compose.setContent { Hosted { FolderPicker("/missing", online.value, "Fixture computer", load = { loads++; throw java.io.IOException("This folder no longer exists.") }, select = { fail("Unavailable folder must not be selected"); it }) {
            fail("A folder that failed to load can't be chosen")
        } } }
        compose.waitUntil { compose.onAllNodesWithText("This folder no longer exists.").fetchSemanticsNodes().size == 1 }
        compose.onNodeWithText("Use this folder").assertIsNotEnabled()
        compose.runOnUiThread { online.value = false }
        compose.onNodeWithText("Connect to this computer to browse its folders.").assertIsDisplayed()
        assertEquals(1, loads)
    }
}

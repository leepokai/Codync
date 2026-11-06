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
    private val online = AppState(loading = false, computers = listOf(Computer("fixture", "Fixture computer", "fixture")),
        actualConnection = LinkState.Ready(HostRoute.Direct))
    private val model = hasContentDescription("Model:", substring = true)

    @Composable private fun Picker(state: AppState, backend: String, selected: String?, choose: (String?) -> Unit, load: suspend (String) -> AgentModels) =
        AgentModelPicker(state, load, backend, selected, emptyMap(), false, null, { id, _ -> choose(id) }, {})

    @Test fun hostModelsRetainAnUnknownOverrideAndFoldTheRecommendedDefault() {
        var selected by mutableStateOf<String?>("old-model")
        compose.setContent { Hosted { Picker(online, "agent", selected, { selected = it }) {
            AgentModels(listOf(AgentModel("auto", "Default (recommended)"), AgentModel("host-model", "Host model", "From this agent")))
        } } }
        compose.waitUntil { compose.onAllNodes(model).fetchSemanticsNodes().size == 1 }
        compose.onNodeWithContentDescription("Model: old-model").performClick()
        compose.onAllNodesWithText("Default (recommended)").assertCountEquals(1)
        compose.onNodeWithText("Default (recommended)").performClick()
        assertNull(selected)
        compose.onNodeWithContentDescription("Model: Default (recommended)").performClick()
        compose.onNodeWithText("Host model").performClick()
        assertEquals("host-model", selected)
        compose.onNodeWithContentDescription("Refresh model list").assertIsDisplayed()
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
        compose.waitUntil { compose.onAllNodes(model).fetchSemanticsNodes().size == 1 }
        compose.waitUntil(3_000) { returned.get() }
        compose.onNode(model).performClick()
        compose.onNodeWithText("new advertised model").assertIsDisplayed()
        compose.onNodeWithText("old advertised model").assertDoesNotExist()
    }

    @Test fun searchKeepsDefaultAndSelectedModelReachableAndCanResetToDefault() {
        var selected by mutableStateOf<String?>("host-model")
        compose.setContent { Hosted { Picker(online, "agent", selected, { selected = it }) {
            AgentModels(listOf(AgentModel("auto", "Default (recommended)"),
                AgentModel("host-model", "Host model"), AgentModel("other", "Other model")))
        } } }
        compose.waitUntil { compose.onAllNodes(model).fetchSemanticsNodes().size == 1 }
        compose.onNodeWithContentDescription("Model: Host model").performClick()
        compose.onNode(hasSetTextAction()).performTextInput("no-match")
        compose.onNodeWithText("Default (recommended)").assertIsDisplayed()
        compose.onNode(hasText("Host model") and isSelected()).assertIsDisplayed()
        compose.onNodeWithText("Other model").assertDoesNotExist()
        compose.onNodeWithText("Default (recommended)").performClick()
        compose.runOnIdle { assertNull(selected) }
        compose.onNodeWithContentDescription("Model: Default (recommended)").performClick()
        compose.onNodeWithText("Other model").assertIsDisplayed()
    }

    @Test fun anOfflineModelOverrideRemainsEditableWithoutInventingAProviderList() {
        var selected by mutableStateOf<String?>("my-model")
        compose.setContent { Hosted { Picker(online.copy(actualConnection = LinkState.Connecting), "agent", selected, { selected = it }) {
            fail("Offline must not query models"); AgentModels(emptyList())
        } } }
        compose.onNodeWithText("Connect to this computer to load its models. You can still enter a model ID.").assertIsDisplayed()
        compose.onNodeWithContentDescription("Retry model list").assertIsDisplayed()
        compose.onNode(model).assertDoesNotExist()
        compose.onNode(hasSetTextAction() and hasText("my-model")).performTextReplacement("saved-model")
        assertEquals("saved-model", selected)
    }

    @Test fun hostFolderBrowsingFiltersLocallyDrillsInAndSelectsTheAuthoritativeDirectory() {
        var chosen: String? = null
        val requests = mutableListOf<String?>()
        compose.setContent { Hosted { FolderPicker("/home", true, { path ->
            requests.add(path)
            if (path == "/home/Repo") DirListing("/home/Repo", "/home", true, emptyList())
            else DirListing("/home", "/", false, listOf(DirItem("Repo", "/home/Repo", true), DirItem("Other", "/home/Other")))
        }) { chosen = it } } }
        compose.waitUntil { compose.onAllNodesWithText("Repo").fetchSemanticsNodes().size == 1 }
        compose.onNode(hasSetTextAction()).performTextInput("repo")
        compose.onNodeWithText("Other").assertDoesNotExist()
        assertEquals(listOf<String?>("/home"), requests)
        compose.onNodeWithText("Repo").performClick()
        compose.waitUntil { compose.onAllNodesWithText("Use “Repo”").fetchSemanticsNodes().size == 1 }
        assertEquals(listOf<String?>("/home", "/home/Repo"), requests)
        compose.onNodeWithText("Use “Repo”").performClick()
        assertEquals("/home/Repo", chosen)
        compose.onNodeWithContentDescription("Back").performClick()
        compose.waitUntil { compose.onAllNodesWithText("Other").fetchSemanticsNodes().size == 1 }
        compose.onNodeWithText("Use “home”").performClick()
        assertEquals("/home", chosen)
    }

    @Test fun anUnavailableOrOfflineFolderShowsWhyWithoutOfferingIt() {
        val online = mutableStateOf(true)
        var loads = 0
        compose.setContent { Hosted { FolderPicker("/missing", online.value, { loads++; throw java.io.IOException("This folder no longer exists.") }) {
            fail("A folder that failed to load can't be chosen")
        } } }
        compose.waitUntil { compose.onAllNodesWithText("This folder no longer exists.").fetchSemanticsNodes().size == 1 }
        compose.onNodeWithText("Use “missing”").assertDoesNotExist()
        compose.runOnUiThread { online.value = false }
        compose.onNodeWithText("Connect to this computer to browse its folders.").assertIsDisplayed()
        assertEquals(1, loads)
    }
}

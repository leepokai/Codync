package com.codync.android

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import com.codync.android.core.Computer
import com.codync.android.core.HostRoute
import com.codync.android.core.LinkState
import com.codync.android.core.VersionMismatch
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** Which side must update before a chat works: the card that stands in for the composer (iOS `UpdateNeededCard`). */
class UpdateNeededCardTest {
    @get:Rule val compose = createComposeRule()
    private val computer = Computer("update-test-0000000001", "Mac fixture", "macos")

    private fun state(canUpdate: Boolean = true, hostUpdate: HostUpdate? = null) = AppState(loading = false,
        computers = listOf(computer), actualConnection = LinkState.Ready(HostRoute.Direct), hostUpdate = hostUpdate,
        hello = buildJsonObject {
            put("name", computer.name); put("version", "99.0.0"); put("os", "macos"); put("canUpdate", canUpdate)
        })

    @Test fun anAppThatIsBehindSaysSoAndNamesTheVersionTheComputerNeeds() {
        compose.setContent { Hosted { UpdateNeededCard(state(), VersionMismatch.UpdateApp("99.0.0"), {}) } }
        compose.onNodeWithText("Update this app").assertIsDisplayed()
        compose.onNodeWithText("needs this app to be 99.0.0 or newer", substring = true).assertIsDisplayed()
        compose.onNodeWithText("Update now").assertDoesNotExist()
    }

    @Test fun aComputerThisAppCanUpdateOffersTheUpdate() {
        var asked = false
        compose.setContent { Hosted { UpdateNeededCard(state(), VersionMismatch.UpdateHost("2.20.0", "2.26.0"), { asked = true }) } }
        compose.onNodeWithText("Update Codync on Mac fixture").assertIsDisplayed()
        compose.onNodeWithText("runs Codync 2.20.0; this app needs 2.26.0 or newer", substring = true).assertIsDisplayed()
        compose.onNodeWithText("Update now").performClick()
        assertTrue(asked)
    }

    @Test fun aComputerThisAppCantUpdateSaysHowToUpdateItThere() {
        compose.setContent { Hosted { UpdateNeededCard(state(canUpdate = false), VersionMismatch.UpdateHost("2.20.0", "2.26.0"), {}) } }
        compose.onNodeWithText("Open Codync on that Mac and choose Check for Updates", substring = true).assertIsDisplayed()
        compose.onNodeWithText("Update now").assertDoesNotExist()
    }

    @Test fun anUpdateUnderWayShowsTheWaitAndOffersNothing() {
        compose.setContent { Hosted {
            UpdateNeededCard(state(hostUpdate = HostUpdate.Requested(System.currentTimeMillis())), VersionMismatch.UpdateHost("2.20.0", "2.26.0"), {})
        } }
        compose.onNodeWithText("Updating Codync on Mac fixture. This takes a few minutes.", substring = true).assertIsDisplayed()
        compose.onNodeWithText("Update now").assertDoesNotExist()
    }
}

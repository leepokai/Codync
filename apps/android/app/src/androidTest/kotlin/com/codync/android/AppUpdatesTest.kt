package com.codync.android

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.core.content.edit
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.HostRoute
import com.codync.android.core.LinkState
import com.codync.android.core.VersionMismatch
import com.google.android.play.core.appupdate.testing.FakeAppUpdateManager
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Rule
import org.junit.Test

/** What Google Play says about this app, through Play's own stand-in (a debug build isn't installed from Play). */
class AppUpdatesTest {
    @get:Rule val compose = createComposeRule()
    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    private fun play(versionCode: Int?) = runBlocking {
        AppUpdates.ask(FakeAppUpdateManager(context).apply { versionCode?.let(::setUpdateAvailable) })
    }

    @After fun nothingNewer() {
        play(null)
        context.getSharedPreferences("codync-updates", android.content.Context.MODE_PRIVATE).edit { clear() }
    }

    @Test fun aNewerReleaseOnGooglePlayShowsAReminderUntilDismissed() {
        play(99_000_000)
        compose.setContent { Hosted { AppUpdateReminder() } }
        compose.onNodeWithText("Codync 99.0.0 is available").assertIsDisplayed()
        compose.onNodeWithText("You have $APP_RELEASE.").assertIsDisplayed()
        compose.onNodeWithContentDescription("Dismiss").performClick()
        compose.onNodeWithText("Codync 99.0.0 is available").assertDoesNotExist()
    }

    @Test fun nothingNewerOnGooglePlayShowsNoReminder() {
        play(null)
        compose.setContent { Hosted { AppUpdateReminder() } }
        compose.onNodeWithText("is available", substring = true).assertDoesNotExist()
    }

    @Test fun updateCodyncWaitsWhileGooglePlayLacksTheVersionTheComputerNeeds() {
        val state = AppState(previewComputer, loading = false, actualConnection = LinkState.Ready(HostRoute.Direct))
        play(null) // Play has only this release; the computer needs a newer one.
        compose.setContent { Hosted { UpdateNeededCard(state, VersionMismatch.UpdateApp("99.0.0"), {}) } }
        compose.onNodeWithText("Update Codync").assertIsNotEnabled()
        compose.onNodeWithText("still in Google Play review", substring = true).assertIsDisplayed()
        play(99_000_000)
        compose.onNodeWithText("Update Codync").assertIsEnabled()
        compose.onNodeWithText("still in Google Play review", substring = true).assertDoesNotExist()
    }
}

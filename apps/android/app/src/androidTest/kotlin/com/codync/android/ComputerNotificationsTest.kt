package com.codync.android

import android.content.Context
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Assert.*
import org.junit.Assume.assumeFalse
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test

/**
 * The app's own notification switch, in Settings: permission first, and the row that follows once
 * Android has been asked.
 */
class ComputerNotificationsTest {
    @get:Rule val compose = createComposeRule()
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val prefs = context.getSharedPreferences("codync-push", Context.MODE_PRIVATE)
    private val previous = PushPreferences.asked(context)

    @Before fun neverAsked() {
        prefs.edit().putBoolean("asked", false).commit()
    }

    @After fun restore() {
        prefs.edit().putBoolean("asked", previous).commit()
    }

    @Test fun withoutPermissionTheRowAsksAndShowsNoSwitch() {
        assumeFalse(notificationsAllowed(context))
        compose.setContent { Hosted { NotificationsSection() } }
        compose.onAllNodes(isToggleable()).assertCountEquals(0)
        compose.onNodeWithText("Turn on").assertIsDisplayed()
    }

    @Test fun settingsRemembersPermissionDeniedFromAnotherEntryPoint() {
        assumeFalse(notificationsAllowed(context))
        // The computer row and onboarding persist the same denial marker.
        PushPreferences.markAsked(context)
        compose.setContent { Hosted { NotificationsSection() } }
        compose.onNodeWithText("Off in Settings").assertIsDisplayed()
        compose.onNodeWithText("Turn on").assertDoesNotExist()
    }

    @Test fun allowedPermissionReadsOn() {
        assumeTrue(notificationsAllowed(context))
        compose.setContent { Hosted { NotificationsSection() } }
        compose.onNodeWithText("On").assertIsDisplayed()
    }
}

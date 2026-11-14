package com.codync.android

import android.content.Context
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assume.assumeFalse
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class ComputerNotificationsTest {
    @get:Rule val compose = createComposeRule()

    @Test fun missingPermissionOpensPermissionWithoutChangingComputerPreference() {
        var opened = false
        var saved = true
        compose.setContent { Hosted {
            ComputerNotificationsControl(saved, false, { saved = it }, { opened = true })
        } }
        compose.onAllNodes(isToggleable()).assertCountEquals(0)
        compose.onNodeWithText("Turn on notifications in Settings").performClick()
        assertTrue(opened)
        assertTrue(saved)
    }

    @Test fun settingsRemembersPermissionDeniedFromAnotherEntryPoint() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        assumeFalse(notificationsAllowed(context))
        val prefs = context.getSharedPreferences("codync-push", Context.MODE_PRIVATE)
        val previous = PushPreferences.asked(context)
        try {
            // The computer row and onboarding persist the same denial marker.
            PushPreferences.markAsked(context)
            compose.setContent { Hosted { NotificationsSection() } }
            compose.onNodeWithText("Off in Settings").assertIsDisplayed()
            compose.onNodeWithText("Turn on").assertDoesNotExist()
        } finally {
            prefs.edit().putBoolean("asked", previous).commit()
        }
    }

    @Test fun permissionChangesRestoreTheSavedPreferenceAndKeepTheSwitchEditable() {
        var allowed by mutableStateOf(false)
        var saved by mutableStateOf(false)
        compose.setContent { Hosted {
            ComputerNotificationsControl(saved, allowed, { saved = it }, {})
        } }
        compose.runOnIdle { allowed = true }
        compose.onNode(isToggleable()).assertIsOff().performClick().assertIsOn()
        compose.runOnIdle { allowed = false }
        compose.onAllNodes(isToggleable()).assertCountEquals(0)
        assertTrue(saved)
        compose.runOnIdle { allowed = true }
        compose.onNode(isToggleable()).assertIsOn()
    }
}

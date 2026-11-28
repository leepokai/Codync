package com.codync.android

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import com.codync.android.core.Entry
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.fail
import org.junit.Rule
import org.junit.Test

/** The approval card's two draws: a stub has no answers, and a full pending row does. */
class PermissionCardRenderTest {
    @get:Rule val compose = createComposeRule()

    @Test fun aStubShowsThePlaceholderAndNoAnswers() {
        compose.setContent {
            Hosted { PermissionCard(stub(), "Fixture computer", answering = false) { fail("a stub must not answer") } }
        }
        compose.onNodeWithText("Wants to run a command").assertIsDisplayed()
        compose.onNodeWithText("Large message kept in storage (40000 characters).").assertIsDisplayed()
        compose.onNodeWithText("Allow once").assertDoesNotExist()
        compose.onNodeWithText("Deny").assertDoesNotExist()
    }

    @Test fun aFullPendingRowShowsAllowAndDeny() {
        var chosen: String? = null
        compose.setContent {
            Hosted { PermissionCard(full(), "Fixture computer", answering = false) { chosen = it } }
        }
        compose.onNodeWithText("Allow once").assertIsDisplayed().performClick()
        compose.onNodeWithText("Deny").assertIsDisplayed()
        compose.onNodeWithText("Large message kept in storage (40000 characters).").assertDoesNotExist()
        assertEquals("allow", chosen)
    }

    private fun stub() = Entry("stub", 1, "bot", rev = 1, kind = "permission", data = buildJsonObject {
        put("storedOnly", JsonPrimitive(true))
        put("text", STORED_STUB_ID)
        put("storedChars", 40_000)
        put("status", "pending")
        put("toolKind", "execute")
        put("title", "npm test")
        put("options", answers())
    })

    private fun full() = Entry("full", 1, "bot", rev = 1, kind = "permission", data = buildJsonObject {
        put("status", "pending")
        put("toolKind", "execute")
        put("title", "npm test")
        put("options", answers())
    })

    private fun answers() = buildJsonArray {
        add(buildJsonObject { put("optionId", "allow"); put("name", "Allow"); put("kind", "allow_once") })
        add(buildJsonObject { put("optionId", "deny"); put("name", "Deny"); put("kind", "reject_once") })
    }
}

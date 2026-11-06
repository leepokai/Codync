package com.codync.android

import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.unit.dp
import com.codync.android.design.LinkButton
import org.junit.Rule
import org.junit.Test

class ConversationKeyboardTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()

    @Test fun conversationTapClearsFocusButButtonTapKeepsIt() {
        compose.activity.setContent {
            Hosted {
                Column {
                    val draft = remember { mutableStateOf("Draft") }
                    BasicTextField(draft.value, { draft.value = it }, Modifier.testTag("composer"))
                    ConversationList("message", 1, false, PaddingValues(0.dp)) {
                        item { Text("Conversation background") }
                        item { LinkButton("Keep editing", action = {}) }
                    }
                }
            }
        }
        compose.onNodeWithTag("composer").performClick().assertIsFocused()
        compose.onNodeWithText("Keep editing").performTouchInput { click() }
        compose.onNodeWithTag("composer").assertIsFocused()
        compose.onNodeWithText("Conversation background").performTouchInput { click() }
        compose.onNodeWithTag("composer").assertIsNotFocused().assertTextEquals("Draft")
    }
}

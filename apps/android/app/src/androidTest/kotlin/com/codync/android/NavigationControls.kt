package com.codync.android

import androidx.compose.runtime.Composable
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.ComposeTestRule
import com.codync.android.design.CodyncTheme
import com.codync.android.design.OverlayRoot

internal fun SemanticsNodeInteractionsProvider.onControl(label: String) = onNode(hasText(label) or hasContentDescription(label))

/** A screen as the app hosts it: the theme, and the overlay its sheets, menus and dialogs draw in. */
@Composable internal fun Hosted(content: @Composable () -> Unit) = CodyncTheme { OverlayRoot(content) }

/** The roster header's computers menu ("Computers, 1 connected. Filter conversations"). */
internal fun ComposeTestRule.computerAction(label: String) {
    onNodeWithContentDescription("Computers,", substring = true).performClick()
    onNodeWithText(label).performClick()
}

/** The button labeled [label] level with [text]: for list rows that merge no semantics of their own. */
internal fun ComposeTestRule.onRowButton(label: String, text: String): SemanticsNodeInteraction {
    val row = onNodeWithText(text).performScrollTo().fetchSemanticsNode().boundsInRoot
    val buttons = onAllNodesWithContentDescription(label)
    val index = buttons.fetchSemanticsNodes().indexOfFirst { it.boundsInRoot.top < row.bottom && it.boundsInRoot.bottom > row.top }
    check(index >= 0) { "No $label button beside $text" }
    return buttons[index]
}

/** Extensions opens from the paired computer's row in Computer management. */
internal fun ComposeTestRule.openExtensions() {
    computerAction("Manage computers")
    waitUntil(10_000) { onAllNodesWithText("Extensions").fetchSemanticsNodes().size == 1 }
    onNodeWithText("Extensions").performClick()
}

/** The open chat's title ("<bot>, view details"): it opens the bot's details, a group's editor. */
internal val chatTitle = hasContentDescription(", view details", substring = true)

internal fun ComposeTestRule.openChatDetails() {
    waitUntil(10_000) { onAllNodes(chatTitle).fetchSemanticsNodes().size == 1 }
    onNode(chatTitle).performClick()
}

/** The roster row's touch-and-hold menu for the bot or group named [name] (the row's label; its text isn't merged in). */
internal fun ComposeTestRule.rosterActions(name: String) {
    val row = hasContentDescription(name) and hasClickAction()
    waitUntil(10_000) { onAllNodes(row).fetchSemanticsNodes().size == 1 }
    onNode(row).performTouchInput { longClick() }
}

/** [label] in the roster's touch-and-hold menu for [name]. */
internal fun ComposeTestRule.rosterAction(name: String, label: String) {
    rosterActions(name)
    onNodeWithText(label).performClick()
}

package com.codync.android

import android.app.Application
import androidx.activity.ComponentActivity
import androidx.compose.runtime.Composable
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.AndroidComposeTestRule
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.Computer
import com.codync.android.design.CodyncTheme
import com.codync.android.design.OverlayRoot
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking

internal fun SemanticsNodeInteractionsProvider.onControl(label: String) = onNode(hasText(label) or hasContentDescription(label))

/** A screen as the app hosts it: the theme, and the overlay its sheets, menus and dialogs draw in. */
@Composable internal fun Hosted(content: @Composable () -> Unit) = CodyncTheme { OverlayRoot(content) }

/** The computer screens rendered on their own show. */
internal val previewComputer = Computer("fixture-computer-00001", "Fixture computer", "fixture")

/** A store no account opened, for a screen rendered on its own: it stays loading, so its actions never run. */
internal fun previewStore(computer: Computer = previewComputer): AppStore {
    val application = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as Application
    return AppStore(application, LocalStore(application), computer, "local", 0, SupervisorJob(), {}, {})
}

/**
 * Runs [block] with the account store the activity's screens get, [computers] opened in it beside the account's own,
 * so a screen rendered on its own finds a store for each computer it shows. They aren't saved, nothing links them
 * while no activity is in the foreground, and closing them afterwards deletes the mirrors they opened.
 */
internal fun AndroidComposeTestRule<*, out ComponentActivity>.withPreviewAccounts(vararg computers: Computer, block: (AccountStore) -> Unit) {
    val activity = activity
    val accounts = runBlocking(Dispatchers.Main) {
        accountStore(activity).also { accounts ->
            accounts.state.first { !it.loading }
            computers.forEach { accounts.open(it) }
        }
    }
    try { block(accounts) }
    finally { runBlocking(Dispatchers.Main) { computers.forEach { accounts.close(it.id) } } }
}

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

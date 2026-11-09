package com.codync.android

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import com.codync.android.core.BotDestination
import com.codync.android.design.*

/**
 * Text shared from another app (`ShareActivity`; the iPhone's Send Text to Bot with Open Chat on): pick a bot and its
 * chat opens with the text in its message box, after anything typed there, for you to send.
 */
@Composable internal fun ShareToBot(text: String, states: List<AppState>, accounts: AccountStore) {
    val dismiss = LocalDismissModal.current
    Column {
        ModalHeader("Send to a bot")
        CardForm {
            CardSection(footer = "The message waits in the chat's box until you send it.") {
                row { Text(text, style = Type.footnote, color = Palette.secondary, maxLines = 4, overflow = TextOverflow.Ellipsis) }
            }
            for (state in states) {
                val bots = roster(state.bots).filter { it.kind != "group" || it.members.isNotEmpty() }
                if (bots.isNotEmpty()) CardSection(if (states.size > 1) state.hostName else null) {
                    for (bot in bots) row {
                        Box(Modifier.plainButton {
                            accounts.sendShared(BotDestination(state.contextId, state.computer.id, bot.id))
                            dismiss()
                        }) { BotRow(bot, state) }
                    }
                }
            }
        }
    }
}

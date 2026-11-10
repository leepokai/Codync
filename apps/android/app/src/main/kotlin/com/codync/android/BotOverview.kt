package com.codync.android

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.design.*

/** The bot's setup at a glance, as the desktop editor shows it. */
@Composable internal fun BotOverview(bot: Bot) {
    Column(Modifier.padding(horizontal = 20.dp)) {
        CardSection("Overview") {
            row { ValueRow("Agent", bot.backend.ifBlank { "—" }) }
            row { ValueRow("Model", bot.model?.takeIf(String::isNotBlank) ?: "Default") }
            row { ValueRow("Folder", bot.cwd) }
            row { ValueRow("Permissions", if (bot.permission == "auto") "Approves automatically" else "Asks before acting") }
            row { ValueRow("Skills", bot.skills.size.toString()) }
            row { ValueRow("Connectors", bot.connectors.size.toString()) }
            row { ValueRow("Computer", if (bot.computer) "Can use it" else "Off") }
        }
    }
}

package com.codync.android

import androidx.compose.runtime.Composable
import com.codync.android.design.*

/**
 * Settings' Shortcuts card (iOS `ShortcutLeadSettings`): which lead "Talk to Codync" calls, shown once more than one
 * computer has a lead. A computer whose lead is gone shows as Automatic, which is what the shortcut does then.
 */
@Composable internal fun ShortcutLeadSettings(app: AccountContext, states: List<AppState>, accounts: AccountStore) {
    val leads = leads(states)
    if (leads.size < 2) return
    val chosen = app.shortcutLeadComputer?.takeIf { id -> leads.any { it.first.computer.id == id } }
    CardSection("Shortcuts", footer = "Talk to Codync, in the app icon's shortcuts, calls this lead. Automatic calls the lead of the first computer in your list.") {
        row {
            ValueRow("Lead to call") {
                ChoicePicker<String?>(chosen, listOf(null to tr("Automatic")) + leads.map { (state, bot) -> state.computer.id to "${bot.name} · ${state.hostName}" },
                    accounts::chooseShortcutLead, label = "Lead to call")
            }
        }
    }
}

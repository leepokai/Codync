package com.codync.android

import android.content.Context
import android.content.Intent
import android.util.Log
import androidx.core.content.pm.ShortcutInfoCompat
import androidx.core.content.pm.ShortcutManagerCompat
import androidx.core.graphics.drawable.IconCompat
import com.codync.android.core.Bot
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

// "Talk to Codync" in the app icon's shortcuts: Android's counterpart of the iPhone's Open Live Mode without a bot
// (docs/features/ios-shortcuts.md). It calls a computer's lead (`talkToLead`); Settings picks whose (`ShortcutLeadSettings`).

/** The app icon's "Talk to Codync" shortcut. */
internal object LiveShortcut {
    const val ACTION = "com.codync.android.action.TALK"

    /** Published while a computer is paired; the launcher keeps it until the next account without one. */
    fun publish(context: Context, paired: Boolean) {
        // The launcher refuses shortcuts while the phone is still locked after a restart: the shortcut is a convenience,
        // so the app goes on without it until the next publish.
        runCatching {
            if (!paired) ShortcutManagerCompat.removeDynamicShortcuts(context, listOf(ID))
            else ShortcutManagerCompat.pushDynamicShortcut(context, ShortcutInfoCompat.Builder(context, ID)
                .setShortLabel(context.getString(R.string.talk_shortcut))
                .setIcon(IconCompat.createWithResource(context, com.codync.android.design.R.drawable.sf_waveform))
                // Into Codync's own task and its one MainActivity, as the widgets open it.
                .setIntent(Intent(ACTION).setClass(context, MainActivity::class.java)
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)).build())
        }.onFailure { Log.w("Codync", "Couldn't publish the Talk to Codync shortcut", it) }
    }

    private const val ID = "talk"
}

/** Each computer's lead, in the user's computer order. */
internal fun leads(computers: List<AppState>): List<Pair<AppState, Bot>> =
    computers.mapNotNull { state -> state.bots.firstOrNull { it.lead && !it.deleted }?.let { state to it } }

/** The lead the shortcut calls: the one on the computer picked in Settings, else the first computer's in the user's order that has one (iOS `ShortcutBotQuery.lead`). */
internal fun shortcutLead(computers: List<AppState>, chosen: String?): Pair<AppState, Bot>? =
    leads(computers).let { all -> all.firstOrNull { it.first.computer.id == chosen } ?: all.firstOrNull() }

/** Saved per account, like the iPhone's `shortcutLeadComputer`; null is Automatic. */
fun AccountStore.chooseShortcutLead(computerId: String?) {
    mutableState.update { it.copy(shortcutLeadComputer = computerId) }
    contextScope.launch(Dispatchers.IO) { local.saveShortcutLead(computerId) }
}

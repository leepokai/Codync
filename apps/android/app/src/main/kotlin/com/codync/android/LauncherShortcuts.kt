package com.codync.android

import android.content.Context
import android.content.Intent
import androidx.core.content.pm.ShortcutInfoCompat
import androidx.core.content.pm.ShortcutManagerCompat
import androidx.core.graphics.drawable.IconCompat
import com.codync.android.core.Bot
import com.codync.android.core.BotShortcutPlan

/**
 * Launcher shortcuts (long-press the app icon) to the bots pinned in this account and computer. They open the same
 * scoped link as a notification, so a shortcut that outlives its account, computer or bot lands on an
 * "unavailable" message (see `openLink`) and never on another bot. Shortcuts the user pinned to the home screen
 * stay on the launcher after sign-out, so they are disabled there instead of removed.
 */
internal object LauncherShortcuts {
    private const val MAX = 4
    /** What the launcher was last given: a roster update that changes nothing must not redraw faces or call it again. */
    @Volatile private var shown: String? = null

    /** Publish the pinned bots of the shown computer, replacing this app's earlier bot shortcuts and keeping its conversation shortcuts. */
    @Synchronized fun sync(context: Context, scope: String, computerId: String, bots: List<Bot>) {
        val limit = minOf(MAX, ShortcutManagerCompat.getMaxShortcutCountPerActivity(context))
        val wanted = BotShortcutPlan.plan(scope, computerId, bots, limit)
        val signature = wanted.joinToString("\u0000") { "${it.id}\u0001${it.label}" }
        if (signature == shown) return
        // A launcher that refuses the list (rate limit, too many, a transient store failure) must not crash the app.
        try {
            val stale = ours(context).filter { id -> wanted.none { it.id == id } }
            if (stale.isNotEmpty()) ShortcutManagerCompat.removeDynamicShortcuts(context, stale)
            val shortcuts = wanted.mapNotNull { plan ->
                val bot = bots.firstOrNull { it.id == plan.botId } ?: return@mapNotNull null
                val members = bot.members.mapNotNull { id -> bots.firstOrNull { it.id == id } }
                ShortcutInfoCompat.Builder(context, plan.id).setShortLabel(plan.label).setLongLabel(bot.name)
                    .setIcon(IconCompat.createWithBitmap(notificationFace(context, bot, members)))
                    // Long-lived, with a share intent last: the Sharesheet offers the bot beside the app icon, as
                    // iOS's share sheet does, and a tap sends the content straight to that bot.
                    .setLongLived(true)
                    .setIntents(arrayOf(NotificationDisplay.open(context, scope, computerId, bot.id), BotShares.intent(context, plan.id)))
                    .build()
            }
            if (shortcuts.isNotEmpty()) ShortcutManagerCompat.addDynamicShortcuts(context, shortcuts)
            shown = signature
        } catch (error: Exception) {
            android.util.Log.w("Codync", "Launcher shortcuts couldn't be published", error)
        }
    }

    /** Sign-out: nothing of this account stays in the launcher's long-press menu, and pinned copies stop opening bots. */
    @Synchronized fun retire(context: Context) {
        shown = null
        try {
            val ids = ours(context)
            ShortcutManagerCompat.removeDynamicShortcuts(context, ids)
            if (ids.isNotEmpty()) ShortcutManagerCompat.disableShortcuts(context, ids, "Sign in to Codync to open this bot.")
        } catch (error: Exception) {
            // The shortcuts' links already fail closed (`openLink` says the bot is unavailable).
            android.util.Log.w("Codync", "Launcher shortcuts couldn't be retired", error)
        }
    }

    private fun ours(context: Context): List<String> =
        ShortcutManagerCompat.getShortcuts(context, ShortcutManagerCompat.FLAG_MATCH_DYNAMIC or ShortcutManagerCompat.FLAG_MATCH_PINNED)
            .map { it.id }.filter { it.startsWith(BotShortcutPlan.PREFIX) }.distinct()
}

package com.codync.android

import android.content.Context
import android.content.Intent
import androidx.core.content.pm.ShortcutManagerCompat
import com.codync.android.core.BotShortcutPlan

/**
 * Sending to an agent from another app's share sheet, without opening Codync first: the pinned agents
 * are long-lived launcher shortcuts that also carry a share intent, so Android's Sharesheet shows
 * them beside the app (what iOS does with its share sheet). A tap is a plain `ACTION_SEND` with the
 * shortcut's id, which names the agent.
 */
internal object BotShares {
    /** The share intent of a shortcut; the last intent of a long-lived shortcut is the one the Sharesheet starts. */
    fun intent(context: Context, shortcutId: String): Intent =
        Intent(context, MainActivity::class.java).apply {
            action = Intent.ACTION_SEND
            type = "text/plain"
            addCategory(Intent.CATEGORY_DEFAULT)
            putExtra(ShortcutManagerCompat.EXTRA_SHORTCUT_ID, shortcutId)
        }

    /** The agent a Sharesheet tap named, when the shortcut is still one of ours (a share from elsewhere names none). */
    fun botOf(intent: Intent): String? = intent.getStringExtra(ShortcutManagerCompat.EXTRA_SHORTCUT_ID)
        ?.takeIf { it.startsWith(BotShortcutPlan.PREFIX) }
        ?.substringAfterLast('/')
        ?.takeIf { it.isNotEmpty() }
}

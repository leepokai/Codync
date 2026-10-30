package com.codync.android

import android.content.Context
import com.codync.android.core.CustomIcon

/** A notification can use only its active account's paired-computer cache. No network or chat reads. */
internal fun notificationIcons(context: Context, scope: String, computer: String): Map<String, CustomIcon> {
    if (PushPreferences.retired(context)) return emptyMap()
    val user = PushPreferences.activeUser(context) ?: "local"
    if (LocalStore.referenceId(user) != scope) return emptyMap()
    return try {
        val local = LocalStore(context, user)
        if (local.computers().none { it.id == computer }) return emptyMap()
        MirrorDatabase(context, local.directory, computer).use { it.icons() }
    } catch (error: Exception) {
        android.util.Log.w("Codync", "Notification uses its built-in face because the image cache is unavailable", error)
        emptyMap()
    }
}

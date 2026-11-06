package com.codync.android

import android.content.Context
import android.graphics.Bitmap
import com.codync.android.core.CustomIcon
import com.codync.android.design.AvatarPalette
import com.codync.android.design.characterAvatarBitmap
import com.codync.android.design.customAvatarBitmap

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

/** A bot's face in a notification (a task's, a call's): its own image, else its character. */
internal fun notificationFace(shape: String, color: String, icon: CustomIcon?): Bitmap =
    customAvatarBitmap(icon?.dataUrl, icon?.tinted == true, 128, AvatarPalette.colors[color]?.toInt() ?: 0xFF1084FE.toInt())
        ?: characterAvatarBitmap(shape, color, 128, 0xFF141414.toInt())

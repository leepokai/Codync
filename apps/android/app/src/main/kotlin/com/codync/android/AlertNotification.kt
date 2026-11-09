package com.codync.android

import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import androidx.core.app.NotificationCompat
import androidx.core.app.Person
import androidx.core.content.ContextCompat
import androidx.core.content.pm.ShortcutInfoCompat
import androidx.core.content.pm.ShortcutManagerCompat
import androidx.core.graphics.drawable.IconCompat
import com.codync.android.core.AlertConversation
import com.codync.android.core.Bot
import com.codync.android.design.characterAvatarBitmap
import com.codync.android.design.customAvatarBitmap
import com.codync.android.design.AvatarPalette
import androidx.compose.ui.graphics.toArgb
import com.codync.android.design.symbolRes

/**
 * A bot's alert as a conversation (iOS `NotificationService.fromBot`): the bot's face and name in
 * place of the app icon, kept in the conversation section by a long-lived shortcut; in a group,
 * the group's face with the member speaking as the sender.
 */
internal object AlertConversations {
    /** [id] names the conversation: the alert's notification tag, one per account, computer and chat. */
    fun style(context: Context, builder: NotificationCompat.Builder, id: String, computer: String, open: Intent, body: String,
        conversation: AlertConversation) {
        val chat = conversation.chat
        val chatFace = IconCompat.createWithBitmap(notificationFace(context, chat, conversation.members))
        val speaker = conversation.speaker
        val sender = if (speaker == null) person(computer, chat, chatFace)
            else person(computer, speaker, IconCompat.createWithBitmap(notificationFace(context, speaker, emptyList())))
        val style = NotificationCompat.MessagingStyle(Person.Builder().setName("You").setKey("me").build())
            .addMessage(body, System.currentTimeMillis(), sender)
        if (speaker != null) style.setConversationTitle(chat.name).setGroupConversation(true)
        val shortcut = ShortcutInfoCompat.Builder(context, id).setShortLabel(chat.name.ifBlank { context.getString(R.string.app_name) })
            .setIcon(chatFace).setIntent(open).setLongLived(true).setIsConversation()
            .apply { if (speaker == null) setPerson(sender) }.build()
        ShortcutManagerCompat.pushDynamicShortcut(context, shortcut)
        builder.setStyle(style).setShortcutInfo(shortcut)
    }

    /** An account's conversations leave with it: bot names and faces never outlive the account on this phone. */
    fun forget(context: Context) {
        val kept = ShortcutManagerCompat.getShortcuts(context, ShortcutManagerCompat.FLAG_MATCH_DYNAMIC or ShortcutManagerCompat.FLAG_MATCH_CACHED)
        ShortcutManagerCompat.removeLongLivedShortcuts(context, kept.map { it.id })
    }

    private fun person(computer: String, bot: Bot, face: IconCompat) =
        Person.Builder().setKey("$computer/${bot.id}").setName(bot.name).setIcon(face).build()
}

// iOS draws the alert face at 88 pt with 20 pt around it on the dark background, at 2x.
private const val FACE_CANVAS = 256
private const val FACE = FACE_CANVAS * 88 / 128
private const val DARK_GROUND = 0xFF0A0A0A.toInt()
private const val DARK_INK = 0xFFF2F2F2.toInt()
private const val DARK_SECONDARY = 0xFF9A9A9A.toInt()

/** The bot's face, or a group's members' faces, on a dark disc like iOS's alert avatars (iOS `GroupAvatar` layouts). */
internal fun notificationFace(context: Context, bot: Bot, members: List<Bot>): Bitmap {
    val bitmap = Bitmap.createBitmap(FACE_CANVAS, FACE_CANVAS, Bitmap.Config.ARGB_8888)
    val canvas = Canvas(bitmap).apply {
        drawCircle(FACE_CANVAS / 2f, FACE_CANVAS / 2f, FACE_CANVAS / 2f, Paint(Paint.ANTI_ALIAS_FLAG).apply { color = DARK_GROUND })
    }
    val start = (FACE_CANVAS - FACE) / 2f
    val end = start + FACE
    fun place(member: Bot, fraction: Float, x: Float, y: Float) {
        val pixels = (FACE * fraction).toInt()
        val face = customAvatarBitmap(member.customIcon?.dataUrl, member.customIcon?.tinted == true, pixels,
            AvatarPalette.tint(member.avatarColor).toArgb())
            ?: characterAvatarBitmap(member.avatarShape, member.avatarColor, pixels, DARK_INK, cells = 13)
        canvas.drawBitmap(face, x, y, null)
    }
    if (bot.kind != "group" || bot.customIcon != null) { place(bot, 1f, start, start); return bitmap }
    // An alert carries at most three members' faces: the first two and the speaker.
    when (members.size) {
        0 -> ContextCompat.getDrawable(context, requireNotNull(symbolRes("person.2")))?.mutate()?.run {
            val side = (FACE * 0.4f).toInt()
            val inset = ((FACE_CANVAS - side) / 2f).toInt()
            setBounds(inset, inset, inset + side, inset + side); setTint(DARK_SECONDARY); draw(canvas)
        }
        1 -> place(members[0], 1f, start, start)
        2 -> { place(members[1], .66f, end - FACE * .66f, start); place(members[0], .66f, start, end - FACE * .66f) }
        else -> {
            place(members[0], .55f, start + FACE * (1 - .55f) / 2, start)
            place(members[1], .55f, start, end - FACE * .55f); place(members[2], .55f, end - FACE * .55f, end - FACE * .55f)
        }
    }
    return bitmap
}

package com.codync.android

import android.content.ClipData
import android.content.ContentResolver
import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.core.content.IntentCompat
import com.codync.android.core.Bot
import com.codync.android.core.MAX_FILE_BYTES
import java.nio.ByteBuffer
import java.nio.CharBuffer
import java.nio.charset.CodingErrorAction
import kotlinx.serialization.Serializable

// Something another app shared and the bots it can go to (`ShareToBot` shows them; the iPhone's Share extension).

/** What another app shared (`ShareActivity`): its text and links, and its files as `content://` links. */
data class SharedContent(val text: String, val uris: List<Uri>) {
    val isEmpty get() = text.isBlank() && uris.isEmpty()

    /**
     * What goes as `pasted` with [message] (iOS `IncomingShare.pasted`): the shared text as the message starts out while the
     * message still holds it, so memory never takes it for the user's own words; nothing once it's edited or deleted, so
     * words the user took out never leave the phone.
     */
    fun pasted(message: String): List<String> = listOfNotNull(fitMessage(text).takeIf { it.isNotBlank() && message.contains(it) })

    /**
     * This share for `MainActivity` in Codync's own task: the files go in its clip with their read grant, since the
     * sender's ends with `ShareActivity`.
     */
    fun forward(context: Context): Intent = Intent(context, MainActivity::class.java).setAction(Intent.ACTION_SEND)
        .putExtra(Intent.EXTRA_TEXT, text).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP).apply {
            if (uris.isEmpty()) return@apply
            clipData = ClipData.newRawUri(null, uris.first()).apply { uris.drop(1).forEach { addItem(ClipData.Item(it)) } }
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }

    companion object {
        /** The most files one share takes, as the photo pickers do (and the iPhone's share extension). */
        const val MAX_FILES = 10

        /**
         * What [intent] shares: another app's `SEND` / `SEND_MULTIPLE`, or [forward]'s. The text is one, or a list of them
         * (styled text arrives as a CharSequence, which getStringExtra drops). The files are the stream extra (one, or a
         * list); without one, the clip's, except for a text share, whose clip is its link preview's thumbnail.
         */
        fun from(intent: Intent, ownPackage: String): SharedContent {
            val text = (intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.let(::listOf)
                ?: intent.getCharSequenceArrayListExtra(Intent.EXTRA_TEXT).orEmpty()).joinToString("\n").trim()
            val streams = listOfNotNull(IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java)) +
                IntentCompat.getParcelableArrayListExtra(intent, Intent.EXTRA_STREAM, Uri::class.java).orEmpty()
            val clip = intent.clipData?.takeIf { intent.type?.startsWith("text/") != true }
            val uris = streams.ifEmpty { clip?.let { (0 until it.itemCount).mapNotNull { index -> it.getItemAt(index).uri } }.orEmpty() }
            return SharedContent(text, files(uris, ownPackage))
        }

        /**
         * The `content://` ones of [uris], each once, at most [MAX_FILES]. None of Codync's own ([ownPackage]'s providers):
         * Codync reads those without a grant, so any app could otherwise hand it a private file of its own to send.
         */
        private fun files(uris: List<Uri>, ownPackage: String) = uris.filter { uri ->
            uri.scheme == "content" && uri.authority.let { it != null && it != ownPackage && !it.startsWith("$ownPackage.") }
        }.distinct().take(MAX_FILES)
    }
}

/** One shared file as the share sheet lists it; [size] is null when its app doesn't say. */
internal class IncomingFile(val uri: Uri, val name: String, val size: Long?, val image: Boolean) {
    val tooLarge get() = size != null && size > MAX_FILE_BYTES

    companion object {
        /** [uri]'s name, size and kind from its app; a link it can't read now fails later, when it's copied in. */
        fun describe(resolver: ContentResolver, uri: Uri): IncomingFile {
            val (name, size) = runCatching { AttachmentFiles.describe(resolver, uri) }.getOrDefault("file" to null)
            val image = runCatching { resolver.getType(uri) }.getOrNull()?.startsWith("image/") == true
            return IncomingFile(uri, name, size, image)
        }
    }
}

/** A chat something was shared to: the share sheet's Recent (iOS `ShareRecipient`). */
@Serializable data class ShareRecipient(val computerId: String, val botId: String)

/** How many chats Recent keeps (iOS `ShareRecipient.kept`). */
private const val RECENT = 3

/** [to] first, then the chats shared to before it, at most [RECENT]. */
internal fun List<ShareRecipient>.recorded(to: ShareRecipient) = (listOf(to) + filter { it != to }).take(RECENT)

/** A bot the share sheet offers, on its computer. */
internal class ShareChoice(val state: AppState, val bot: Bot) {
    val recipient get() = ShareRecipient(state.computer.id, bot.id)
}

/** Recent's bots still listed (most recent first), then each computer's other bots in roster order (groups with members). */
internal fun shareChoices(states: List<AppState>, recipients: List<ShareRecipient>): Pair<List<ShareChoice>, List<Pair<AppState, List<ShareChoice>>>> {
    val listed = states.map { state -> state to roster(state.bots).filter { it.kind != "group" || it.members.isNotEmpty() }.map { ShareChoice(state, it) } }
    val all = listed.flatMap { it.second }
    val recent = recipients.mapNotNull { wanted -> all.firstOrNull { it.recipient == wanted } }
    return recent to listed.map { (state, choices) -> state to choices.filter { it !in recent } }
}

/** [text] cut at a character's edge to the most a message holds (`DraftBook.TEXT_BYTES` of UTF-8). */
internal fun fitMessage(text: String): String {
    val input = CharBuffer.wrap(text)
    // The encoder stops before the first character that doesn't fit, never inside one; a lone surrogate counts as the '?' it's saved as.
    Charsets.UTF_8.newEncoder().onMalformedInput(CodingErrorAction.REPLACE).encode(input, ByteBuffer.allocate(DraftBook.TEXT_BYTES), true)
    return text.substring(0, input.position())
}

internal const val SHARE_FAILED = "Couldn't send. Try again."

package com.codync.android

import android.net.Uri
import androidx.lifecycle.viewModelScope
import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*

// Content other apps share to Codync: staged here until the user picks a bot, then it becomes that bot's draft.

/** Shared text and files, already copied into this account's private storage. Never sent on its own. */
data class IncomingShare(val text: String, val files: List<OutgoingFile>)

/**
 * A system share arrived (`ACTION_SEND` / `ACTION_SEND_MULTIPLE`). Files are copied while the provider's grant is
 * alive; then the user chooses the bot, or [botId] sends it straight to the agent the Sharesheet named. A share meant
 * for an account that has since been replaced, or one an older share finished after, is discarded instead of
 * replacing what is staged.
 */
fun AppStore.receiveShare(text: String, uris: List<Uri>, botId: String? = null) {
    val asked = shareRequests.incrementAndGet()
    viewModelScope.launch {
        val activated = withTimeoutOrNull(30_000) { activated() }
        if (activated == null) { mutableState.update { it.copy(error = "Couldn't open the shared content yet. Open Codync and share it again.") }; return@launch }
        val generation = activated.generation
        val imported = mutableListOf<OutgoingFile>()
        try {
            for (uri in uris.take(ShareIntake.MAX_FILES)) {
                require(uri.scheme == "content") { "Share files from Photos or Files." }
                imported += attachments.import(uri)
            }
            check(state.value.generation == generation) { "The account changed before the shared content was ready. Share it again." }
            // A newer share, a cancellation or a leaving account took over while these files were copying.
            check(asked == shareRequests.get()) { "A newer share replaced this one." }
            clearShare()
            mutableState.update { it.copy(incomingShare = IncomingShare(text, imported.toList())) }
            // The Sharesheet already asked which agent: no picker, the share lands in its draft.
            if (botId != null && state.value.bots.any { it.id == botId }) deliverShare(botId)
        } catch (error: CancellationException) {
            withContext(NonCancellable + Dispatchers.IO) { imported.forEach(attachments::discard) }
            throw error
        } catch (error: Exception) {
            withContext(NonCancellable + Dispatchers.IO) { imported.forEach(attachments::discard) }
            // A superseded share or a leaving account is not a failure the user has to read about.
            if (asked == shareRequests.get()) showError(error)
        }
    }
}

/** Put the staged share in [botId]'s draft (after what's typed there) and open that chat; the user sends it. */
fun AppStore.deliverShare(botId: String) {
    val share = state.value.incomingShare ?: return
    try {
        addShareToDraft(botId, share.text, share.files)
    } catch (error: Exception) {
        // Nothing was applied, so the share stays staged and picking this bot again is safe.
        showError(error)
        return
    }
    mutableState.update { it.copy(incomingShare = null) }
    openBot(botId)
}

/** Drop a staged share the user cancelled, replaced, or that belongs to an account that's leaving, with its copied files. */
internal fun AppStore.discardShare() {
    shareRequests.incrementAndGet()
    clearShare()
}

private fun AppStore.clearShare() {
    val share = state.value.incomingShare ?: return
    mutableState.update { it.copy(incomingShare = null) }
    val files = attachments
    viewModelScope.launch(Dispatchers.IO) { share.files.forEach(files::discard) }
}

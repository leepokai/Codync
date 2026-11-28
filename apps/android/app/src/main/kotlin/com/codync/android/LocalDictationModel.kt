package com.codync.android

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.codync.android.core.LocalDictation
import kotlinx.coroutines.CancellationException

/**
 * Whether the computer has a speech server linked for dictation with its own speech model, and the computer's
 * message when none is: asked when the computer (re)connects. [fetch] is the computer's answer, a parameter so a
 * test can stand in for it, and [host] is its name in the messages said when it can't be asked.
 */
internal class LocalDictationModel(private val host: String, private val fetch: suspend () -> LocalDictation) {
    /** null while the computer hasn't answered yet. */
    var local by mutableStateOf<LocalDictation?>(null)
        private set
    var error by mutableStateOf<String?>(null)
        private set

    suspend fun load(online: Boolean) {
        local = null
        error = null
        if (!online) { error = "Not connected to $host."; return }
        try { local = fetch() } catch (cancelled: CancellationException) { throw cancelled }
        catch (failed: Exception) { error = failed.message ?: "Couldn't reach $host." }
    }
}

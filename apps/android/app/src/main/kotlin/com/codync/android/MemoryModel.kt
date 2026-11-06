package com.codync.android

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.codync.android.core.BotMemory
import com.codync.android.core.MemoryCalls
import com.codync.android.core.MemoryFileDetail
import com.codync.android.core.WireJson
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.decodeFromJsonElement

/** One bot's memory on the computer (iOS `MemoryStore`): fetches it, and changes it through the host. */
internal class MemoryModel(private val botId: String, private val store: AppStore, private val scope: CoroutineScope) {
    var view by mutableStateOf<BotMemory?>(null); private set
    var busy by mutableStateOf(false); private set
    var error by mutableStateOf<String?>(null)
    private var request = 0

    suspend fun load() {
        val asked = ++request
        try {
            val value = WireJson.decodeFromJsonElement<BotMemory>(store.hostCall("memory", MemoryCalls.read(botId)))
            if (asked == request) { view = value; error = null }
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) {
            // A host before 2.18.0 answers with its old memory listing, which has no layers or dreams.
            if (asked == request) error = if (failure is SerializationException || failure is IllegalArgumentException)
                "Update Codync on your computer to see this bot's memory." else failure.message ?: "Couldn't load memory."
        }
    }

    fun refresh() = scope.launch { load() }

    /** Drops the error of an action that failed once the sheet has read the latest; what the user wrote stays. */
    fun dismissError() { error = null }

    /** A long-form note as it is now, with its versions. */
    suspend fun file(name: String, shared: Boolean): MemoryFileDetail? = try {
        WireJson.decodeFromJsonElement<MemoryFileDetail>(store.hostCall("memoryFile", MemoryCalls.fileRead(botId, name, shared)))
    } catch (cancelled: CancellationException) { throw cancelled }
    catch (failure: Exception) { error = failure.message ?: "Couldn't load the note."; null }

    /** Saves, deletes (`content` and `version` null) or brings back a version of a note over what was loaded (`base`); reloads after it. */
    suspend fun changeFile(name: String, shared: Boolean, base: String?, content: String? = null, version: Long? = null): Boolean {
        if (busy) return false
        busy = true; error = null
        return try {
            store.hostCall(MemoryCalls.fileMethod(content, version), MemoryCalls.fileChange(botId, name, shared, base, content, version))
            load()
            true
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message ?: "Something went wrong."; false }
        finally { busy = false }
    }

    /** A memory method for this bot; reloads after it. */
    fun mutate(method: String, id: Long? = null, pinned: Boolean? = null, archived: Boolean? = null, shared: Boolean? = null,
        text: String? = null, done: () -> Unit = {}) {
        if (busy) return
        busy = true; error = null
        scope.launch {
            try {
                store.hostCall(method, MemoryCalls.mutation(botId, id, pinned, archived, shared, text))
                load()
                done()
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) { error = failure.message ?: "Something went wrong." }
            finally { busy = false }
        }
    }
}

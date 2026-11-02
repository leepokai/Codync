package com.codync.android

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.codync.android.core.BotMemory
import com.codync.android.core.MemoryFileDetail
import com.codync.android.core.WireJson
import kotlinx.coroutines.CancellationException
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.put

/** One bot's memory, read-only: the bot changes it (iOS `MemoryStore`). */
internal class MemoryModel(private val botId: String, private val store: AppStore) {
    var memory by mutableStateOf<BotMemory?>(null); private set
    var error by mutableStateOf<String?>(null)
    private var request = 0

    suspend fun load() {
        val asked = ++request
        try {
            val value = WireJson.decodeFromJsonElement<BotMemory>(store.hostCall("memory", buildJsonObject { put("botId", botId) }))
            if (asked == request) { memory = value; error = null }
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) {
            // A host before 2.18.0 answers with its old memory listing, which has no layers or dreams.
            if (asked == request) error = if (failure is SerializationException || failure is IllegalArgumentException)
                "Update Codync on your computer to see this bot's memory." else failure.message ?: "Couldn't load memory."
        }
    }

    /** A long-form note as it is now. */
    suspend fun file(name: String, shared: Boolean): MemoryFileDetail? = try {
        WireJson.decodeFromJsonElement<MemoryFileDetail>(store.hostCall("memoryFile", buildJsonObject {
            put("botId", botId); put("file", name); put("shared", shared)
        }))
    } catch (cancelled: CancellationException) { throw cancelled }
    catch (failure: Exception) { error = failure.message ?: "Couldn't open the note."; null }
}

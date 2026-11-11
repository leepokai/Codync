package com.codync.android

import com.codync.android.core.*
import kotlinx.coroutines.flow.update
import kotlinx.serialization.json.*

// Threads: a side conversation opened from a chat, so one message can be followed without filling the chat.

/** A chat's live threads, or its archive drawer, newest first; the host never sends their lines. */
internal fun AppStore.refreshThreads(botId: String, archived: Boolean = false) {
    action("threads:$botId:$archived") {
        val response = hostQuery("threads", buildJsonObject { put("botId", botId); put("archived", archived) })
        val list = ThreadInfo.decodeList(response)
        mutableState.update {
            if (archived) it.copy(archivedThreads = it.archivedThreads + (botId to list)) else it.copy(threads = it.threads + (botId to list))
        }
    }
}

/** Opens a thread, optionally branched from one of the chat's messages, and shows it. */
fun AppStore.startThread(botId: String, fromEntryId: String? = null, title: String = "") = action("new-thread:$botId") {
    val response = hostQuery("createThread", buildJsonObject {
        put("botId", botId)
        put("title", title)
        fromEntryId?.let { put("fromEntryId", it) }
    })
    val id = response.jsonObject.getValue("threadId").jsonPrimitive.content
    if (state.value.selectedBot != botId) stopReading()
    val read = chatRead("thread", body("botId" to botId, "rootId" to id))
    receiveEntries(read)
    val info = (read.jsonObject["thread"] as? JsonObject)?.let { ThreadInfo.decode(it) }
    mutableState.update { it.copy(selectedBot = botId, thread = id, threadInfo = info, trace = false, removedComputer = false) }
    refreshThreads(botId)
}

fun AppStore.renameThread(threadId: String, title: String) = action("rename-thread:$threadId") {
    require(title.isNotBlank()) { "A thread needs a name." }
    hostQuery("renameThread", body("threadId" to threadId, "title" to title.trim()))
    mutableState.update { state ->
        state.copy(threadInfo = state.threadInfo?.takeIf { it.id == threadId }?.copy(title = title.trim()) ?: state.threadInfo)
    }
    state.value.selectedBot?.let { refreshThreads(it) }
}

fun AppStore.archiveThread(threadId: String, archived: Boolean = true) = action("archive-thread:$threadId") {
    hostQuery("archiveThread", buildJsonObject { put("threadId", threadId); put("archived", archived) })
    mutableState.update { state ->
        state.copy(threadInfo = state.threadInfo?.takeIf { it.id == threadId }?.copy(archived = archived) ?: state.threadInfo)
    }
    state.value.selectedBot?.let { refreshThreads(it); refreshThreads(it, archived = true) }
}

/** Removes a thread and its replies; the chat keeps everything else. */
fun AppStore.deleteThread(threadId: String) = action("delete-thread:$threadId") {
    hostQuery("deleteThread", body("threadId" to threadId))
    val open = state.value.thread
    mutableState.update { it.copy(threadInfo = it.threadInfo?.takeIf { info -> info.id != threadId }) }
    if (open == threadId) state.value.selectedBot?.let { openBot(it) }
    state.value.selectedBot?.let { refreshThreads(it); refreshThreads(it, archived = true) }
}

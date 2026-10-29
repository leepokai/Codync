package com.codync.android

import java.io.File
import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.serialization.json.*

// Artifacts bots publish (docs/features/artifacts.md): a chat's Library, each shown artifact's HTML (read again once a newer
// version is known), renaming and sharing.

private fun AppStore.updateArtifacts(change: (ArtifactCache) -> ArtifactCache) = mutableState.update { it.copy(artifacts = change(it.artifacts)) }

/** The `artifact` events in a batch: details only, a card or viewer showing the artifact reads its new HTML. */
internal fun AppStore.receiveArtifacts(events: List<MirrorEvent>) {
    val artifacts = events.filterIsInstance<MirrorEvent.ArtifactChanged>().map { it.artifact }
    if (artifacts.isNotEmpty()) updateArtifacts { cache -> artifacts.fold(cache, ArtifactCache::upsert) }
}

/**
 * A chat's Library, fetched when it shows: its artifacts' details join the artifact cache (events keep them current while
 * connected), its files stay listed for that chat.
 */
fun AppStore.loadLibrary(botId: String) = action("library:$botId") {
    val response = ready().call("library", body("botId" to botId)).jsonObject
    val items = response.getValue("items").jsonArray.mapNotNull { (it as? JsonObject)?.let(LibraryItem::decode) }
    val artifacts = items.filterIsInstance<LibraryItem.ArtifactItem>().map { it.artifact }
    mutableState.update { it.copy(library = it.library + (botId to items), artifacts = artifacts.fold(it.artifacts, ArtifactCache::upsert)) }
}

/**
 * Reads an artifact's HTML unless the newest version known (`announced`: the one its card names) is here already; one
 * published while it was read is read next. A failure stays with the artifact, for its card and viewer to show.
 */
fun AppStore.loadArtifact(botId: String, id: String, announced: Long = 0) {
    if (!state.value.artifacts.needsRead(id, announced)) return
    action("artifact:$id") {
        updateArtifacts { it.copy(failures = it.failures - id) }
        try {
            readArtifact(botId, id)
            if (state.value.artifacts.needsRead(id)) readArtifact(botId, id)
        } catch (error: CancellationException) { throw error }
        catch (error: Exception) { updateArtifacts { it.fail(id, error.message ?: "Couldn't read the artifact.") } }
    }
}

private suspend fun AppStore.readArtifact(botId: String, id: String): String {
    val (artifact, html) = HostFiles(ready()).readArtifact(botId, id)
    updateArtifacts { it.store(artifact, html) }
    return html
}

fun AppStore.renameArtifact(botId: String, id: String, title: String) = action("rename-artifact:$id") {
    val response = ready().call("renameArtifact", buildJsonObject { put("botId", botId); put("artifactId", id); put("title", title) }).jsonObject
    updateArtifacts { it.upsert(Artifact.decode(response.getValue("artifact"))) }
}

/** The system share sheet with the artifact's newest HTML as `<title>.html`. */
fun AppStore.shareArtifact(botId: String, id: String, title: String) = action("share-artifact:$id") {
    val cache = state.value.artifacts
    val html = cache.html[id]?.takeUnless { cache.needsRead(id) }?.text ?: readArtifact(botId, id)
    val file = withContext(Dispatchers.IO) {
        File(attachments.shareFolder().apply { mkdirs() }, artifactFileName(title)).apply { writeText(html) }
    }
    attachments.open(file, file.name)
}

/** `<title>.html`, a name the share sheet can hand out: no folders, at most 200 bytes. */
internal fun artifactFileName(title: String): String {
    var name = title.replace('/', '-').replace('\\', '-')
    while (name.toByteArray().size > MAX_NAME_BYTES - ".html".length) name = name.dropLast(1)
    return AttachmentFiles.cleanName("$name.html")
}

private const val MAX_NAME_BYTES = 200

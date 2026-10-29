package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.decodeFromJsonElement

/**
 * A self-contained HTML document a bot published for the user (`host/src/chat/artifacts.rs`). Its HTML comes from
 * `readArtifact`; publishing again under the same id replaces it and moves `updatedAt`.
 */
@Serializable data class Artifact(
    val id: String,
    val botId: String,
    val title: String,
    val size: Long,
    val sha256: String = "",
    val createdAt: Long = 0,
    val updatedAt: Long = 0,
    /** Its newest card in the bot's chat, what a message about the artifact replies to; null when no card was posted
     *  (a routine run, a group chat or another bot's request). */
    val entryId: String? = null,
) {
    companion object {
        const val MAX_BYTES = 5L * 1024 * 1024

        fun decode(value: JsonElement): Artifact = WireJson.decodeFromJsonElement<Artifact>(value).also {
            require(it.id.isNotBlank() && it.botId.isNotBlank() && it.size in 0..MAX_BYTES) { "The computer sent an invalid artifact." }
        }
    }
}

/** How a message about artifact `artifactId` quotes its card (`entryId`) until the computer's own quote arrives. */
fun artifactQuote(entryId: String, botId: String, artifactId: String, title: String) = ReplyQuote(entryId, botId, "Artifact: $title", artifactId)

/** The draft of the message box under an artifact, apart from its chat's (deleting the bot drops both). */
fun artifactDraftKey(botId: String, artifactId: String) = "$botId/artifact/$artifactId"

/** The first host with artifacts and the Library. */
const val ARTIFACTS_HOST = "2.25.0"

/** Whether the computer's host has the Library; an older one has no `library` method, so the app leaves it out. */
fun hasLibrary(hostVersion: String?): Boolean = hostVersion != null && !AppVersion.isBelow(hostVersion, ARTIFACTS_HOST)

/** What an artifact card in the chat carries (`data.artifact`): the artifact and the version it announced. */
@Serializable data class ArtifactCard(val id: String, val title: String = "", val updatedAt: Long = 0)

/** An artifact's HTML as last read: `updatedAt` names the version it is. */
data class ArtifactHtml(val updatedAt: Long, val text: String)

/**
 * One computer's artifacts: their details by id (from `library`, `readArtifact` and the `artifact` events) and the HTML of the
 * ones shown, read again once a newer version is known.
 */
data class ArtifactCache(
    val details: Map<String, Artifact> = emptyMap(),
    val html: Map<String, ArtifactHtml> = emptyMap(),
    /** Why an artifact's HTML couldn't be read, until it's read again. */
    val failures: Map<String, String> = emptyMap(),
) {
    /** Older details never replace newer ones; a rename keeps `updatedAt`, so it does. */
    fun upsert(artifact: Artifact): ArtifactCache {
        val known = details[artifact.id]
        return if (known != null && known.updatedAt > artifact.updatedAt) this else copy(details = details + (artifact.id to artifact))
    }

    fun store(artifact: Artifact, text: String): ArtifactCache =
        upsert(artifact).copy(html = html + (artifact.id to ArtifactHtml(artifact.updatedAt, text)), failures = failures - artifact.id)

    fun fail(id: String, message: String): ArtifactCache = copy(failures = failures + (id to message))

    /** The newest version of an artifact known here; `announced` is the version a card names. */
    fun newest(id: String, announced: Long = 0): Long = maxOf(announced, details[id]?.updatedAt ?: 0)

    /** The HTML held is older than the newest version known, or missing. */
    fun needsRead(id: String, announced: Long = 0): Boolean = (html[id]?.updatedAt ?: -1) < newest(id, announced)

    /** A card's title: the artifact's own once it is as new as the card. */
    fun title(card: ArtifactCard): String = details[card.id]?.takeIf { it.updatedAt >= card.updatedAt }?.title ?: card.title

    /** A chat's Library: its bot's artifacts as known here (events keep them current) with the files `listed` names, newest first. */
    fun library(botId: String, listed: List<LibraryItem>): List<LibraryItem> =
        (details.values.filter { it.botId == botId }.map { LibraryItem.ArtifactItem(it.updatedAt, it) } + listed.filter { it !is LibraryItem.ArtifactItem })
            .sortedByDescending { it.at }
}

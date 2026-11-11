package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.jsonObject

/** The GitHub account of the computer's host, as `githubStatus` reports it. Never carries the token. */
@Serializable data class GitHubStatus(
    val configured: Boolean = false,
    val login: String? = null,
    val scopes: List<String> = emptyList(),
)

/** What the GitHub settings card shows. */
object GitHub {
    /** `github_pat_…` or a classic token: 20–255 letters, digits, `-` or `_`. */
    fun tokenOk(token: String): Boolean {
        val value = token.trim()
        return value.length in 20..255 && value.all { it in 'a'..'z' || it in 'A'..'Z' || it in '0'..'9' || it == '_' || it == '-' }
    }

    fun decode(response: JsonElement): GitHubStatus =
        WireJson.decodeFromJsonElement(GitHubStatus.serializer(), response.jsonObject)
}

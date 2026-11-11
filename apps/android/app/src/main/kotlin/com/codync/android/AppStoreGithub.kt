package com.codync.android

import com.codync.android.core.GitHub
import com.codync.android.core.GitHubStatus
import kotlinx.serialization.json.buildJsonObject

/** The GitHub account of the computer's host. Null from an older host. */
suspend fun AppStore.githubStatus(): GitHubStatus? = runCatching { GitHub.decode(hostQuery("githubStatus", buildJsonObject { })) }.getOrNull()

suspend fun AppStore.disconnectGitHub(): GitHubStatus = GitHub.decode(hostQuery("disconnectGitHub", buildJsonObject { }))

/** Gives the host a GitHub token; it answers with the account that token belongs to. */
suspend fun AppStore.setGitHubToken(token: String): GitHubStatus =
    GitHub.decode(hostQuery("setGitHubToken", body("token" to token.trim()), timeoutMillis = 60_000))

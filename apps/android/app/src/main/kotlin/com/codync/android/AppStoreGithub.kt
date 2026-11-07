package com.codync.android

import com.codync.android.core.GitHub
import com.codync.android.core.GitHubStatus
import kotlinx.serialization.json.buildJsonObject

/** The GitHub account of the computer's host. Null from an older host. */
suspend fun AppStore.githubStatus(): GitHubStatus? = runCatching { GitHub.decode(hostQuery("githubStatus", buildJsonObject { })) }.getOrNull()

suspend fun AppStore.disconnectGitHub(): GitHubStatus = GitHub.decode(hostQuery("disconnectGitHub", buildJsonObject { }))

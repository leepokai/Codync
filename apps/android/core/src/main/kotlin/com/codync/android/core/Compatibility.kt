package com.codync.android.core

/**
 * Which side must update before this app and a host can work together
 * (docs/reference/compatibility.md): each names the oldest version of the other it works with.
 */
sealed interface VersionMismatch {
    /** This app is older than the host's `minApp`. */
    data class UpdateApp(val minimum: String) : VersionMismatch
    /** The host is older than `MIN_HOST`. */
    data class UpdateHost(val version: String, val minimum: String) : VersionMismatch

    companion object {
        /** The oldest host this app works with; raise it only when a core flow needs something older hosts lack. */
        const val MIN_HOST = "2.26.0"

        /** An unknown version is never a reason to lock someone out. */
        fun check(app: String, hostVersion: String?, minApp: String?, minHost: String = MIN_HOST): VersionMismatch? = when {
            hostVersion == null -> null
            minApp != null && AppVersion.isBelow(app, minApp) -> UpdateApp(minApp)
            AppVersion.isBelow(hostVersion, minHost) -> UpdateHost(hostVersion, minHost)
            else -> null
        }
    }
}

object AppVersion {
    /** `2.9.0` < `2.14.1`, numerically, ignoring a `-dev` style suffix; unreadable versions compare as not below. */
    fun isBelow(version: String, minimum: String): Boolean {
        val have = parse(version) ?: return false
        val need = parse(minimum) ?: return false
        for (i in 0 until maxOf(have.size, need.size)) {
            val a = have.getOrElse(i) { 0 }
            val b = need.getOrElse(i) { 0 }
            if (a != b) return a < b
        }
        return false
    }

    private fun parse(version: String): List<Int>? = version.substringBefore('-').split('.').map { it.toIntOrNull() ?: return null }
}

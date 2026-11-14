package com.codync.android.core

import java.net.URI

data class UsageDestination(val scope: String, val computerId: String) {
    companion object {
        fun parse(value: String, scheme: String): UsageDestination {
            val uri = URI(value)
            require(uri.scheme == scheme && uri.rawAuthority == "usage" && uri.fragment == null && uri.path.orEmpty() in setOf("", "/"))
            val scoped = BotDestination.parse("$scheme://bot/usage?" + uri.rawQuery.orEmpty(), scheme)
            return UsageDestination(scoped.scope, scoped.computerId)
        }
    }
}

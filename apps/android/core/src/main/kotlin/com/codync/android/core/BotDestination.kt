package com.codync.android.core

import java.net.URI
import java.net.URLDecoder
import java.nio.charset.StandardCharsets

/** A bot ID alone is never a destination: notifications and widgets retain their account/computer. */
data class BotDestination(val scope: String, val computerId: String, val botId: String) {
    companion object {
        fun parse(value: String, scheme: String): BotDestination {
            val uri = URI(value)
            require(uri.scheme == scheme && uri.rawAuthority == "bot" && uri.fragment == null)
            val path = uri.path
            require(path.startsWith('/') && path.count { it == '/' } == 1)
            val bot = path.drop(1)
            require(bot.isNotBlank() && bot.length <= 128 && bot.none(Char::isISOControl))
            val query = uri.rawQuery.orEmpty().split('&').filter(String::isNotEmpty).map { item ->
                val pair = item.split('=', limit = 2)
                require(pair.size == 2)
                URLDecoder.decode(pair[0], StandardCharsets.UTF_8.name()) to URLDecoder.decode(pair[1], StandardCharsets.UTF_8.name())
            }
            require(query.count { it.first == "scope" } == 1 && query.count { it.first == "computer" } == 1)
            val scope = query.single { it.first == "scope" }.second
            val computer = query.single { it.first == "computer" }.second
            require(scope == "local" || scope.matches(Regex("[a-f0-9]{64}")))
            require(computer.matches(Regex("[A-Za-z0-9_-]{22}")))
            computer.decodeBase64Url(16)
            return BotDestination(scope, computer, bot)
        }
    }
}

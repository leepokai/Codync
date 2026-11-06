package com.codync.android.core

import java.net.URI
import java.net.URLDecoder
import java.security.MessageDigest
import kotlinx.serialization.Serializable

@Serializable data class ConnectorSignIn(val url: String, val callback: String)
@Serializable data class PendingConnectorOAuth(val connectorId: String, val computerId: String,
    val stateHash: String, val expiresAt: Long, val requestId: String? = null) {
    fun validate(back: OAuthReturn, computer: String, now: Long) {
        require(now < expiresAt && expiresAt <= now + 15 * 60_000 && computerId == computer &&
            MessageDigest.isEqual(stateHash.toByteArray(), hash(back.state).toByteArray())) { "This connector sign-in expired or belongs to another computer. Start sign-in again." }
    }
    companion object {
        fun create(plan: ConnectorSignIn, connectorId: String, computerId: String, now: Long): PendingConnectorOAuth {
            val uri = URI(plan.url)
            require(plan.callback == "app" && uri.scheme == "https" && !uri.host.isNullOrBlank() && uri.userInfo == null && uri.fragment == null) { "This connector didn't provide a supported sign-in page." }
            val state = query(uri)["state"]
            require(!state.isNullOrBlank() && state.length <= 2048) { "The connector sign-in page is missing its state." }
            return PendingConnectorOAuth(connectorId, computerId, hash(state), now + 15 * 60_000)
        }
        private fun hash(value: String): String = MessageDigest.getInstance("SHA-256").digest(value.toByteArray())
            .joinToString("") { "%02x".format(it) }
    }
}
data class OAuthReturn(val state: String, val code: String?, val error: String?) {
    companion object {
        fun parse(value: String, scheme: String): OAuthReturn {
            require(value.length <= 16_384)
            val uri = URI(value)
            require(uri.scheme == scheme && uri.host == "oauth" && uri.userInfo == null && uri.port == -1 && uri.path.isNullOrEmpty() && uri.fragment == null)
            val fields = query(uri)
            val state = fields["state"]
            val code = fields["code"]?.takeIf(String::isNotBlank)
            val error = fields["error_description"]?.takeIf(String::isNotBlank) ?: fields["error"]?.takeIf(String::isNotBlank)
            require(!state.isNullOrBlank() && state.length <= 2048 && (code != null) != (error != null))
            return OAuthReturn(state, code, error)
        }
    }
}
private fun query(uri: URI): Map<String, String> {
    val result = mutableMapOf<String, String>()
    uri.rawQuery.orEmpty().split('&').filter(String::isNotBlank).forEach { part ->
        val name = URLDecoder.decode(part.substringBefore('='), "UTF-8")
        val value = URLDecoder.decode(part.substringAfter('=', ""), "UTF-8")
        require(result.put(name, value) == null) { "The sign-in link contains repeated fields." }
    }
    return result
}

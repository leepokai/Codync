package com.codync.android.core

import kotlinx.serialization.json.JsonObject
import org.junit.Assert.assertEquals
import org.junit.Test

class McpAccessTest {
    @Test fun listsTheAppsNotRevokedNewestFirstAndSkipsWhatItCantRead() {
        val now = 1_800_000_000_000L
        val apps = McpApp.active(WireJson.parseToJsonElement("""{"clients":[
            {"id":"a","label":"Laptop agent","access":"auto","createdAt":1,"expiresAt":${now + 3 * 86_400_000}},
            {"id":"b","label":"Old","access":"full","createdAt":2,"expiresAt":$now,"revokedAt":5},
            {"label":"no id"},
            {"id":"c","label":"Odd","access":"root","createdAt":3,"expiresAt":${now - 1},"tokenHash":"x"}
        ],"levels":{"max":"auto"}}"""))
        assertEquals(listOf("c", "a"), apps.map { it.id })
        assertEquals(3, apps[1].daysLeft(now))
        assertEquals("past its expiry it has none left", 0, apps[0].daysLeft(now))
        assertEquals("an unknown level reads as the least", "Read only", apps[0].levelName)
        assertEquals(emptyList<McpApp>(), McpApp.active(JsonObject(emptyMap())))
    }
}

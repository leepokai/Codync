package com.codync.android

import com.codync.android.core.WireJson
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

class ComposerDraftArtifactTest {
    @Test fun artifactAndDocumentDraftsKeepTheirConversationAndTheirOwnSavedText() {
        val chat = ComposerDraft("bot", text = "chat text")
        val document = ComposerDraft("bot", threadId = "doc:page", text = "document text")
        val artifact = ComposerDraft("bot", text = "artifact text", draftKey = "bot/artifact/report")
        assertEquals("bot", artifact.botId)
        assertEquals(null, artifact.threadId)
        assertNotEquals(chat.lane, document.lane)
        assertNotEquals(chat.lane, artifact.lane)
        assertNotEquals(document.lane, artifact.lane)
        val restored = WireJson.decodeFromString<ComposerDraft>(WireJson.encodeToString(artifact))
        assertEquals(artifact, restored)
        assertEquals("bot/artifact/report", restored.lane)
    }
}

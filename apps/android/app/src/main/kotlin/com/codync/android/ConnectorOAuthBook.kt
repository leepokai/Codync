package com.codync.android

import android.util.AtomicFile
import com.codync.android.core.PendingConnectorOAuth
import com.codync.android.core.WireJson
import java.io.File
import kotlinx.serialization.encodeToString

/** One short-lived state hash, excluded from backup; codes and credentials are never saved. */
internal class ConnectorOAuthBook(directory: File) {
    private val file = AtomicFile(File(directory, "connector-oauth.json"))
    fun read(): PendingConnectorOAuth? {
        if (!file.baseFile.exists()) return null
        check(file.baseFile.length() <= 4096) { "The pending connector sign-in is damaged." }
        return WireJson.decodeFromString(file.readFully().toString(Charsets.UTF_8))
    }
    fun save(pending: PendingConnectorOAuth) {
        val output = file.startWrite()
        try { output.write(WireJson.encodeToString(pending).toByteArray()); file.finishWrite(output) }
        catch (error: Throwable) { file.failWrite(output); throw error }
    }
    fun clear() = file.delete()
}

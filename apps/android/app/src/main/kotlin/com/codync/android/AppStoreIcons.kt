package com.codync.android

import android.app.Application
import android.net.Uri
import android.util.Base64
import com.codync.android.core.*
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.*

internal fun AppStore.setIcon(target: String, file: Uri? = null, url: String? = null, tinted: Boolean? = null, remove: Boolean = false) {
    action("icon:$target") {
        require((file == null && url == null) || state.value.account.userId != null) { "Sign in to use custom images." }
        val data = file?.let { uri -> withContext(Dispatchers.IO) {
            val bytes = getApplication<Application>().contentResolver.openInputStream(uri)?.use { input ->
                val output = java.io.ByteArrayOutputStream()
                val buffer = ByteArray(8192)
                while (output.size() <= 5 * 1024 * 1024) {
                    val count = input.read(buffer)
                    if (count < 0) break
                    output.write(buffer, 0, count)
                }
                output.toByteArray()
            }
                ?: error("Cannot read this image.")
            require(bytes.isNotEmpty() && bytes.size <= 5 * 1024 * 1024) { "Choose an image up to 5 MiB." }
            Base64.encodeToString(bytes, Base64.NO_WRAP)
        } }
        val active = ready()
        val mirror = requireNotNull(database)
        val epoch = mirror.iconEpoch()
        val response = active.call("setIcon", buildJsonObject {
            put("target", target)
            data?.let { put("data", it) }
            url?.let { put("url", it) }
            tinted?.let { put("tinted", it) }
            if (remove) put("remove", true)
        }, 60_000).jsonObject
        val icons = decodeCustomIcons(response.getValue("icons"))
        val applied = withContext(Dispatchers.IO) {
            mirror.applyIconReply(MirrorEvent.Icons(icons, response.getValue("iconsRevision").jsonPrimitive.long), epoch)
        }
        if (applied && database === mirror) refresh()
    }
}

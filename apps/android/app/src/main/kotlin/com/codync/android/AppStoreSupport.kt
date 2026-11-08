package com.codync.android

import java.io.File
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

// The Support page's saved log: the recent events as one text file the share sheet can attach.

private val supportFileName = DateTimeFormatter.ofPattern("yyyyMMdd-HHmm")

/** Writes the log to `codync-support-<date>.txt` and hands it to the system share sheet. */
fun AppStore.saveSupportLog() = action("support-log") {
    val stamp = Instant.now().atZone(ZoneId.systemDefault()).format(supportFileName)
    val file = withContext(Dispatchers.IO) {
        File(attachments.shareFolder().apply { mkdirs() }, "codync-support-$stamp.txt").apply { writeText(SupportLog.fileText()) }
    }
    attachments.open(file, file.name)
}

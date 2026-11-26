package com.codync.android.design

import android.content.ClipData
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.LocalClipboard
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** Whether a path was just copied, and a copy that puts a path on the clipboard and says so for a moment ("Path copied"). */
@Composable fun rememberCopyPath(): Pair<Boolean, (String) -> Unit> {
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    var copied by remember { mutableStateOf(false) }
    return copied to { path ->
        scope.launch {
            clipboard.setClipEntry(ClipEntry(ClipData.newPlainText("Path", path)))
            copied = true; delay(1500); copied = false
        }
    }
}

package com.codync.android

import android.graphics.Bitmap
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.codync.android.core.QrPixels
import com.codync.android.core.WhatsApp
import com.codync.android.core.WhatsAppLink
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * The computer's linked WhatsApp number. WhatsApp has no official desktop client API: the number
 * becomes a linked device, and a separate number is safer because WhatsApp can block one it sees as
 * automated. The host runs the link; a phone only shows its state and lets someone link or unlink.
 */
@Composable internal fun WhatsAppSheet(state: AppState, store: AppStore) {
    var link by remember { mutableStateOf<WhatsAppLink?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    // The link changes on its own while a QR code is scanned, so it is read again while this is open.
    LaunchedEffect(state.computers.firstOrNull()?.id) {
        while (true) {
            link = store.whatsapp()
            delay(1_500)
        }
    }
    fun act(work: suspend () -> Unit) {
        if (busy) return
        busy = true; error = null
        scope.launch {
            try { work() } catch (e: Exception) { error = e.message ?: "That did not work." } finally { busy = false }
        }
    }
    val current = link
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader("WhatsApp")
        CardForm {
            CardSection(footer = tr("WhatsApp does not support this kind of client officially and can block a number it sees as automated. A separate number is safer.")) {
                row {
                    Text(tr(WhatsApp.stateText(current?.state.orEmpty())), style = Type.body, color = Palette.text)
                }
                when (current?.state) {
                    "qr" -> current.qr?.let { code ->
                        row {
                            Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally,
                                verticalArrangement = Arrangement.spacedBy(10.dp)) {
                                Text(tr("On your phone, open WhatsApp → Linked devices → Link a device, then scan this code."),
                                    style = Type.body, color = Palette.secondary)
                                QrImage(code)
                            }
                        }
                    }
                    "connected" -> row {
                        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            Text(current.name ?: current.me.orEmpty(), style = Type.body, color = Palette.text)
                            Text(tr("Messages from your allowed chats reach the bot you pick. A workflow can also start from a mention."),
                                style = Type.footnote, color = Palette.secondary)
                        }
                    }
                }
                row {
                    val bots = state.bots.sortedBy { it.name }
                    val choices = listOf("" to tr("No bot")) + bots.map { it.id to it.name }
                    ValueRow("Bot", detail = tr("Messages from your allowed chats reach the bot you pick. A workflow can also start from a mention.")) {
                        ChoicePicker(current?.bot.orEmpty(), choices, { act { store.whatsappSetBot(it.ifEmpty { null }) } }, label = "Bot")
                    }
                }
                current?.error?.let { row { Text(it, style = Type.caption, color = Palette.danger) } }
                row {
                    when (current?.state) {
                        "connected", "qr", "starting" -> SecondaryButton(tr("Unlink"), enabled = !busy) { act { store.whatsappLogout(); link = store.whatsapp() } }
                        else -> PrimaryButton(tr("Link WhatsApp"), enabled = !busy) { act { store.whatsappConnect(); link = store.whatsapp() } }
                    }
                }
            }
            error?.let { Text(it, style = Type.caption, color = Palette.danger) }
        }
    }
}

/** The QR code a linked-device scan reads, with a white plate so dark themes keep it readable. */
@Composable private fun QrImage(text: String) {
    val bitmap = remember(text) {
        val size = 512
        Bitmap.createBitmap(QrPixels.encode(text, size), size, size, Bitmap.Config.ARGB_8888)
    }
    Image(bitmap.asImageBitmap(), contentDescription = "WhatsApp QR code",
        modifier = Modifier.size(232.dp).clip(RoundedRectangle(12.dp)).background(Color.White))
}

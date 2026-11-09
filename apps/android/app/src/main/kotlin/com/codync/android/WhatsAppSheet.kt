package com.codync.android

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
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
    var notice by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    var jid by remember { mutableStateOf("") }
    var phone by remember { mutableStateOf("") }
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    // The link changes on its own while a QR code is scanned, so it is read again while this is open.
    LaunchedEffect(state.computers.firstOrNull()?.id) {
        while (true) {
            link = store.whatsapp()
            delay(1_500)
        }
    }
    // The coroutine that finishes an action runs outside composition, so its words are read now.
    val failed = tr("That did not work.")
    val copied = tr("Code copied.")
    fun act(work: suspend () -> Unit) {
        if (busy) return
        busy = true; error = null; notice = null
        scope.launch {
            try { work() } catch (e: Exception) { error = e.message ?: failed } finally { busy = false }
        }
    }
    fun copyCode(code: String) {
        (context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager)
            .setPrimaryClip(ClipData.newPlainText("WhatsApp pairing code", code))
        notice = copied
    }
    val current = link
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader("WhatsApp")
        CardForm {
            CardSection(footer = tr("WhatsApp does not support this kind of client officially and can block a number it sees as automated. A separate number is safer.")) {
                row {
                    Text(tr(WhatsApp.stateText(current?.state.orEmpty())), style = Type.body, color = Palette.text)
                }
                when {
                    current?.state == "qr" -> current.qr?.let { code ->
                        row {
                            Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally,
                                verticalArrangement = Arrangement.spacedBy(10.dp)) {
                                Text(tr("On your phone, open WhatsApp → Linked devices → Link a device, then scan this code."),
                                    style = Type.body, color = Palette.secondary)
                                QrImage(code)
                            }
                        }
                    }
                    current?.state == "pairing" -> current.pairCode?.let { code ->
                        row {
                            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                Text(tr("On your phone, open WhatsApp → Linked devices → Link a device, choose Link with phone number, then type this code."),
                                    style = Type.body, color = Palette.secondary)
                                // Read one character at a time, as the phone shows the field.
                                Text(WhatsApp.groupedCode(code), style = Type.title2.monospaced, color = Palette.text,
                                    modifier = Modifier.semantics { contentDescription = code.toCharArray().joinToString(" ") })
                            }
                        }
                        row { SecondaryButton(tr("Copy code"), enabled = !busy) { copyCode(code) } }
                    }
                    current?.state == "connected" -> row {
                        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            Text(current.name ?: current.me.orEmpty(), style = Type.body, color = Palette.text)
                            Text(tr("Messages from your allowed chats reach the bot you pick. A workflow can also start from a mention."),
                                style = Type.footnote, color = Palette.secondary)
                        }
                    }
                    current?.state != "starting" -> {
                        row {
                            FieldBox(phone, { phone = it }, "5215555555555", fill = Palette.bubbleUser, capitalize = false)
                        }
                        row {
                            PrimaryButton(tr("Link with a code"), enabled = !busy && phone.isNotBlank()) {
                                act { store.whatsappPair(phone); link = store.whatsapp() }
                            }
                            SecondaryButton(tr("Scan a QR code"), enabled = !busy) {
                                act { store.whatsappConnect(); link = store.whatsapp() }
                            }
                        }
                    }
                }
                row {
                    val bots = state.bots.sortedBy { it.name }
                    val choices = listOf("" to tr("No bot")) + bots.map { it.id to it.name }
                    ValueRow(tr("Bot"), detail = tr("Messages from your allowed chats reach the bot you pick. A workflow can also start from a mention.")) {
                        ChoicePicker(current?.bot.orEmpty(), choices, { act { store.whatsappSetBot(it.ifEmpty { null }) } }, label = tr("Bot"))
                    }
                }
                current?.error?.let { row { Text(it, style = Type.caption, color = Palette.danger) } }
                notice?.let { row { Text(it, style = Type.caption, color = Palette.secondary) } }
                row {
                    when (current?.state) {
                        "connected" -> SecondaryButton(tr("Unlink"), enabled = !busy) { act { store.whatsappLogout(); link = store.whatsapp() } }
                        "qr", "starting", "pairing" -> SecondaryButton(tr("Cancel"), enabled = !busy) { act { store.whatsappLogout(); link = store.whatsapp() } }
                    }
                }
            }
            if (link?.state == "connected") {
                CardSection(tr("Allowed chats"), footer = tr("Anyone can write while this is empty. The owner always can.")) {
                    link?.allowed.orEmpty().forEach { entry ->
                        row {
                            Text(entry, style = Type.caption.monospaced, color = Palette.text, modifier = Modifier.weight(1f))
                            SecondaryButton(tr("Remove"), enabled = !busy) { act { store.whatsappSetAllowed(entry, false); link = store.whatsapp() } }
                        }
                    }
                    row {
                        FieldBox(jid, { jid = it }, "5215555555555@s.whatsapp.net", fill = Palette.bubbleUser, capitalize = false)
                        SecondaryButton(tr("Allow"), enabled = !busy && jid.isNotBlank()) {
                            val value = jid.trim()
                            act { store.whatsappSetAllowed(value, true); jid = ""; link = store.whatsapp() }
                        }
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

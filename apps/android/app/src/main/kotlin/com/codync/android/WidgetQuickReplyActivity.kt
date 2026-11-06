package com.codync.android

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Intent
import android.os.Bundle
import android.speech.RecognizerIntent
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import com.codync.android.core.NotificationActionPlan
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle

/**
 * A short message to one bot, typed or dictated, from the overview widget. A small sheet over the home screen: the message goes through
 * the same durable work as a notification reply, and nothing says "sent" before the computer answered (a failure becomes a notification
 * that keeps the text).
 */
class WidgetQuickReplyActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val botId = intent.getStringExtra(EXTRA_BOT)
        val feed = WidgetStore.feed(this)
        val owner = WidgetStore.owner(this)
        val computer = feed.computerId
        val bot = feed.bots.firstOrNull { it.id == botId }
        if (bot == null || owner == null || computer == null) { finish(); return }
        setContent {
            CodyncTheme {
                OverlayRoot {
                    var text by rememberSaveable { mutableStateOf("") }
                    var problem by remember { mutableStateOf<String?>(null) }
                    val noDictation = tr("Dictation isn't available on this phone.")
                    val cannotSend = tr("That message can't be sent.")
                    val dictate = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
                        if (result.resultCode == Activity.RESULT_OK) {
                            val said = result.data?.getStringArrayListExtra(RecognizerIntent.EXTRA_RESULTS)?.firstOrNull().orEmpty()
                            if (said.isNotBlank()) text = (text.trim() + " " + said).trim()
                        }
                    }
                    Box(Modifier.fillMaxSize().background(Palette.text.copy(alpha = 0.35f)).safeDrawingPadding().imePadding()) {
                        Column(Modifier.align(Alignment.BottomCenter).fillMaxWidth().padding(12.dp).clip(RoundedRectangle(24.dp))
                            .background(Palette.background).padding(18.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                            Text("${tr("Message")} ${bot.name}", style = Type.headline, color = Palette.text, maxLines = 1)
                            FieldBox(text, { text = it.take(NotificationActionPlan.MAX_REPLY) }, tr("Write or dictate a message"), 2, 6, fill = Palette.bubbleUser)
                            problem?.let { Text(it, style = Type.caption, color = Palette.danger) }
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                                IconButton(tr("Dictate"), "mic") {
                                    try {
                                        dictate.launch(Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH)
                                            .putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM))
                                    } catch (_: ActivityNotFoundException) { problem = noDictation }
                                }
                                SecondaryButton(tr("Cancel"), Modifier.weight(1f)) { finish() }
                                PrimaryButton(tr("Send"), Modifier.weight(1f), enabled = text.isNotBlank()) {
                                    val action = NotificationActionPlan.reply(NotificationActionPlan.freshNonce(), owner.scope, computer, bot.id, text)
                                    if (action == null) problem = cannotSend
                                    else { WidgetActionWork.enqueue(this@WidgetQuickReplyActivity, action); finish() }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    companion object { const val EXTRA_BOT = "bot" }
}

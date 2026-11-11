package com.codync.android

import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.text.style.TextOverflow
import com.codync.android.core.*
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

/**
 * Voice chat settings (iOS `VoiceChatSettingsView`): which voice carries calls (Android speech on this
 * phone, or OpenAI / Gemini / ElevenLabs on the user's own key kept by the computer), the mode,
 * models and voice. Opened from a call's gear and from Settings.
 */
@Composable internal fun VoiceChatSettings(state: AppState, store: AppStore) {
    AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.VoiceSettings))
    val context = LocalContext.current
    val choices = remember { store.voiceChoices() }
    var engine by remember { mutableStateOf(choices.provider) }
    var settings by remember { mutableStateOf(VoicePreferences.load(context)) }
    var transcripts by remember { mutableStateOf(VoicePreferences.transcripts(context)) }
    fun change(next: VoiceSettings) { settings = next; store.voiceSettings(next) }
    // Closing hands a running call to a different engine picked here.
    DisposableEffect(Unit) { onDispose { store.switchVoiceEngine() } }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader({
            Symbol("waveform", size = 24.dp, tint = Palette.text)
            Spacer(Modifier.width(8.dp))
            Text(tr("Voice chat"), style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        })
        CardForm {
            val provider = engine
            var speech by remember(provider) { mutableStateOf(provider?.let(choices::speechMode) == true) }
            var localSpeech by remember { mutableStateOf(choices.localSpeech) }
            var wakePhrase by remember { mutableStateOf(choices.wakePhrase) }
            CardSection("Voice", footer = provider?.let { "Your own ${it.title} key, billed by ${it.title}. During a call, applies when you close settings." }
                ?: "Android speech, on this device. During a call, applies when you close settings.", Modifier.animateContentSize(Motion.layoutSize)) {
                row {
                    ValueRow("Engine") {
                        ChoicePicker(provider, listOf<Pair<VoiceProvider?, String>>(null to "On device") + VoiceProvider.entries.map { it to it.title },
                            { engine = it; choices.provider = it }, label = "Engine")
                    }
                }
                if (provider != null) {
                    if (provider.live) row {
                        ValueRow("Mode", detail = if (speech) "Sent when you pause, replies read aloud." else "A live conversation. Talk over replies.") {
                            ChoicePicker(speech, listOf(false to "Realtime", true to "Speech"), { speech = it; choices.speechMode(provider, it) }, label = "Mode")
                        }
                    }
                    if (speech) {
                        row { ValueRow("Speech output", detail = "NeuTTS runs on your computer with its prepared Hermes model. Recognition still uses your selected speech provider.") {
                            ChoicePicker(localSpeech, listOf(false to "Provider voice", true to "NeuTTS local · Jo"), { localSpeech = it; choices.localSpeech = it }, label = "Speech output")
                        } }
                        row { PauseRow(settings, ::change) }
                    }
                } else {
                    row { ValueRow("Wake phrase", detail = "Say Hey Hermes before an on-device turn. Audio stays on this phone. Available only while the call is open and the app is visible.") {
                        ChoicePicker(wakePhrase, listOf(false to "Off", true to "Hey Hermes"), { wakePhrase = it; choices.wakePhrase = it }, label = "Wake phrase")
                    } }
                    row { LanguageRow(store) }
                    row {
                        ValueRow("Reading speed", detail = tr("How fast replies are read aloud.")) {
                            ChoicePicker(settings.rate, readingSpeedOptions, { change(settings.copy(rate = it)) }, label = "Reading speed")
                        }
                    }
                    row { PauseRow(settings, ::change) }
                }
            }
            if (provider != null) key(provider) { ProviderVoiceSettings(provider, speech, state, store, choices) }
            CardSection("Call receipts", "Applies to the next call. Text is stored with this bot on the host. Realtime transcription can add provider charges. Text may be incomplete and does not confirm playback or decisions.") {
                row { SwitchRow("Save call transcripts", null, transcripts, { transcripts = it; VoicePreferences.transcripts(context, it) }) }
            }
        }
    }
}

/** The language on-device calls hear: the phone's, or one Android hears on this phone (English always). */
@Composable private fun LanguageRow(store: AppStore) {
    var language by remember { mutableStateOf(store.callLanguage()) }
    var heard by remember { mutableStateOf(emptyList<String>()) }
    LaunchedEffect(Unit) { heard = store.callLanguages() }
    val collator = remember { java.text.Collator.getInstance() }
    val named = (heard + language).filter { it.isNotEmpty() }.distinct().map { it to PlatformSpeech.name(it) }.sortedWith(compareBy(collator) { it.second })
    ValueRow("Language", detail = "The language you speak in calls.") {
        ChoicePicker(language, listOf("" to "System") + named, { language = it; store.callLanguage(it) }, label = "Language")
    }
}

/** How long a silence ends what you say, before it's sent: a slider, so a pause can be tuned, not only picked. */
@Composable private fun PauseRow(settings: VoiceSettings, change: (VoiceSettings) -> Unit) {
    StepSlider("Send after a pause of", settings.pauseMillis, pauseStops, { change(settings.copy(pauseMillis = it)) },
        detail = tr("Longer leaves time to think mid-sentence."))
}

/** The pauses the slider offers, in seconds. */
private val pauseStops: List<Pair<Long, String>> =
    VoicePause.stops.map { it to String.format(java.util.Locale.US, "%.1f s", it / 1000.0) }

/** The reading speeds offered, in multiples of the normal one. */
private val readingSpeedOptions: List<Pair<Float, String>> = readingSpeeds.map { it to speedLabel(it) }

/**
 * One provider: the key the computer keeps, models, voice and this month's minutes. Keys are entered
 * on the computer itself (the host takes `setVoiceKey` from loopback only).
 */
@Composable private fun ProviderVoiceSettings(provider: VoiceProvider, speech: Boolean, state: AppState, store: AppStore, choices: VoiceChoices) {
    val scope = rememberCoroutineScope()
    val host = state.hostName
    /** null while the computer hasn't answered yet. */
    var saved by remember { mutableStateOf<Boolean?>(null) }
    var busy by remember { mutableStateOf(false) }
    var result by remember { mutableStateOf<Pair<Boolean, String>?>(null) }
    var voice by remember { mutableStateOf(choices.voice(provider)) }
    /** The provider's voices: its own list (ElevenLabs), or the fixed ids until it answers. */
    var voices by remember { mutableStateOf(provider.voices.map { it to it }) }
    var model by remember { mutableStateOf(choices.model(provider, VoiceChoices.Model.Realtime)) }
    var transcribe by remember { mutableStateOf(choices.model(provider, VoiceChoices.Model.Transcribe)) }
    var speechModel by remember { mutableStateOf(choices.model(provider, VoiceChoices.Model.Speech)) }
    var models by remember { mutableStateOf(choices.cachedModels(provider) ?: VoiceModels()) }
    val hasKey = saved == true

    suspend fun loadVoices() {
        if (provider != VoiceProvider.ElevenLabs) return
        val list = try { store.voiceVoices(provider).voices } catch (cancelled: CancellationException) { throw cancelled } catch (_: Exception) { return }
        if (list.isEmpty()) return
        voices = list.map { it.id to it.name }
        if (list.none { it.id == voice }) { voice = list.first().id; choices.voice(provider, voice) }
    }
    suspend fun loadModels() {
        loadVoices()
        val list = try { store.voiceModels(provider) } catch (cancelled: CancellationException) { throw cancelled } catch (_: Exception) { return }
        models = list
        // A newer default moves every choice the user hasn't made themselves.
        choices.remember(list.defaults, provider)
        model = choices.model(provider, VoiceChoices.Model.Realtime)
        transcribe = choices.model(provider, VoiceChoices.Model.Transcribe)
        speechModel = choices.model(provider, VoiceChoices.Model.Speech)
        choices.cache(provider, list)
    }
    LaunchedEffect(provider) {
        if (state.actualConnection !is LinkState.Ready) { saved = false; result = false to "Not connected to $host."; return@LaunchedEffect }
        try {
            val status = store.voiceStatus()
            choices.remember(status); saved = status.configured(provider)
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (error: Exception) { saved = false; result = false to (error.message ?: "Couldn't reach $host."); return@LaunchedEffect }
        if (saved == true) loadModels()
    }
    // With a key saved, Test starts a call credential (or reads "OK" aloud) with the chosen model and voice.
    fun test() {
        if (busy || !hasKey) return
        busy = true; result = null
        scope.launch {
            try {
                val clip = if (speech) store.voiceSpeak(provider, speechModel, voice, "OK", localOutput = choices.localSpeech) else null
                if (clip == null) store.voiceSession(provider, model, voice)
                result = true to "Key works"
                loadModels()
                // Reads "OK" with this voice, so the key's reading is heard, not just checked.
                if (clip != null) AndroidClipPlayer.play(clip.first, clip.second)
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (error: Exception) { result = false to (error.message ?: "The key didn't work.") }
            finally { busy = false }
        }
    }
    @Composable fun picker(label: String, selection: String, choices: List<Pair<String, String>>, select: (String) -> Unit) {
        // The current choice stays listed before the provider's list arrives.
        val options = if (choices.any { it.first == selection }) choices else listOf(selection to selection) + choices
        ValueRow(label, Modifier.graphicsLayer { alpha = if (hasKey) 1f else .55f }) {
            ChoicePicker(selection, options, select, label = label, enabled = hasKey)
        }
    }

    CardSection(provider.title, footer = if (hasKey) null else "Add a key to choose from ${provider.title}'s models.",
        modifier = Modifier.animateContentSize(Motion.layoutSize)) {
        if (saved != null || result != null) row {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (hasKey) Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    Symbol("key.fill", size = 18.dp, tint = Palette.secondary)
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
                        Text("Key saved", style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text("Stays on $host", style = Type.caption, color = Palette.tertiary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                    if (busy) Spinner() else IconButton("Test key", "checkmark.seal", action = ::test)
                } else if (saved == false) Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    Symbol("key", Modifier.padding(top = 2.dp), size = 18.dp, tint = Palette.tertiary)
                    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Text("No ${provider.title} key on $host", style = Type.body, color = Palette.text)
                        Text("Add it in Codync on that computer: Computers & devices, then Voice chat.", style = Type.caption, color = Palette.secondary)
                    }
                }
                result?.let { (ok, text) ->
                    IconLabel(text, if (ok) "checkmark.circle.fill" else "exclamationmark.circle.fill", if (ok) Palette.secondary else Palette.danger)
                }
            }
        }
        if (speech) {
            row { picker("Speech to text", transcribe, models.transcribe.map { it to it }) { transcribe = it; choices.choose(provider, VoiceChoices.Model.Transcribe, it) } }
            row { picker("Text to speech", speechModel, models.speech.map { it to it }) { speechModel = it; choices.choose(provider, VoiceChoices.Model.Speech, it) } }
        } else row { picker("Model", model, models.models.map { it to it }) { model = it; choices.choose(provider, VoiceChoices.Model.Realtime, it) } }
        row { picker("Voice", voice, voices) { voice = it; choices.voice(provider, it) } }
        row { ValueRow("This month", "≈ ${choices.minutes(provider)} min", Modifier.graphicsLayer { alpha = if (hasKey) 1f else .55f }) }
    }
}

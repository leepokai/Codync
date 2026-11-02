package com.codync.android

import android.Manifest
import android.app.*
import android.content.*
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.media.*
import android.net.Uri
import android.os.*
import androidx.core.app.NotificationCompat
import androidx.core.app.Person
import androidx.core.content.ContextCompat
import androidx.core.graphics.drawable.IconCompat
import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine

/** The call the UI shows; `notice` is the line under the bar, e.g. why it fell back to on-device voice. */
data class VoiceCallViewState(val id: String, val botId: String, val state: VoiceState = VoiceState(), val notice: String? = null)

/** Started only by the visible call UI. Audio is retained in background, never restarted by Android. */
class VoiceService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var store: AppStore? = null
    private var session: VoiceBinding? = null
    private var callId: String? = null
    private var focus: AudioFocusRequest? = null
    private lateinit var manager: AudioManager
    private var previousMode: Int? = null
    private var previousDevice: AudioDeviceInfo? = null
    private var selectedDevice: AudioDeviceInfo? = null
    private var previousSpeaker = false
    private var startedSco = false
    private var receivers = false
    private var ending = false
    private val noisy = object : BroadcastReceiver() {
        override fun onReceive(context: Context?, intent: Intent?) { store?.focusVoice(false) }
    }
    private val devices = object : AudioDeviceCallback() {
        override fun onAudioDevicesAdded(addedDevices: Array<out AudioDeviceInfo>) { chooseRoute() }
        override fun onAudioDevicesRemoved(removedDevices: Array<out AudioDeviceInfo>) {
            if (removedDevices.any { it.id == selectedDevice?.id && it.type != AudioDeviceInfo.TYPE_BUILTIN_SPEAKER }) store?.focusVoice(false)
            chooseRoute()
        }
    }
    private val recording = object : AudioManager.AudioRecordingCallback() {
        override fun onRecordingConfigChanged(configs: MutableList<AudioRecordingConfiguration>?) {
            val id = store?.voiceRecordingSession() ?: return
            if (configs.orEmpty().any { it.clientAudioSessionId == id && it.isClientSilenced }) store?.focusVoice(false)
        }
    }
    override fun onCreate() {
        super.onCreate(); manager = getSystemService(AudioManager::class.java)
        getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel(CHANNEL, "Voice chat", NotificationManager.IMPORTANCE_LOW))
    }
    override fun onBind(intent: Intent?): IBinder? = null
    @Suppress("DEPRECATION") override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val id = intent?.getStringExtra(ID)
        if (intent?.action == START) {
            val request = pending?.takeIf { it.id == id }
            if (request == null || session != null) { if (session == null) stopSelf(startId); return START_NOT_STICKY }
            pending = null; store = request.store; callId = request.id; currentId = request.id
            if (request.store.state.value.voice?.id != request.id ||
                ContextCompat.checkSelfPermission(this, Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
                end("Allow microphone access before starting voice chat."); return START_NOT_STICKY
            }
            try {
                startForeground(NOTIFICATION, notification(request.store.state.value.voice!!.state),
                    ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE or ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK)
                val attributes = AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
                    .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build()
                val ownedFocus = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN).setAudioAttributes(attributes)
                    .setOnAudioFocusChangeListener({ change ->
                        when (change) {
                            AudioManager.AUDIOFOCUS_GAIN -> store?.focusVoice(true)
                            AudioManager.AUDIOFOCUS_LOSS -> end("Voice chat ended because another app took audio focus.")
                            else -> store?.focusVoice(false)
                        }
                    }, Handler(Looper.getMainLooper())).build()
                focus = ownedFocus
                check(manager.requestAudioFocus(ownedFocus) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED)
                previousMode = manager.mode
                previousSpeaker = manager.isSpeakerphoneOn
                if (Build.VERSION.SDK_INT >= 31) previousDevice = manager.communicationDevice
                manager.mode = AudioManager.MODE_IN_COMMUNICATION
                chooseRoute()
                manager.registerAudioDeviceCallback(devices, Handler(Looper.getMainLooper()))
                manager.registerAudioRecordingCallback(recording, Handler(Looper.getMainLooper()))
                ContextCompat.registerReceiver(this, noisy, IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY), ContextCompat.RECEIVER_NOT_EXPORTED)
                receivers = true
                val call = request.store.attachVoice(request.id, request.botId) ?: throw IllegalStateException("The active account changed")
                session = call
                scope.launch { combine(call.state, call.progress, ::Pair).collectLatest { (state, progress) ->
                    if (state.phase in listOf(VoicePhase.Failed, VoicePhase.Ended)) end(state.error)
                    else getSystemService(NotificationManager::class.java).notify(NOTIFICATION, notification(state, progress))
                } }
            } catch (_: Exception) { end("Couldn't start voice chat. Check microphone access and Android's audio settings.") }
        } else if (id == callId) {
            when (intent?.action) { MUTE -> store?.muteVoice(); END -> end(null) }
        } else if (session == null) stopSelf(startId)
        return START_NOT_STICKY
    }
    @Suppress("DEPRECATION") private fun chooseRoute() {
        if (previousMode == null || store?.state?.value?.voice?.id != callId) return
        if (Build.VERSION.SDK_INT >= 31) {
            val available = manager.availableCommunicationDevices
            val headset = available.firstOrNull { it.type in listOf(AudioDeviceInfo.TYPE_BLUETOOTH_SCO, AudioDeviceInfo.TYPE_BLE_HEADSET) }
                ?: available.firstOrNull { it.type in listOf(AudioDeviceInfo.TYPE_WIRED_HEADSET, AudioDeviceInfo.TYPE_USB_HEADSET) }
            val chosen = headset ?: available.firstOrNull { it.type == AudioDeviceInfo.TYPE_BUILTIN_SPEAKER }
            if (chosen != null && manager.setCommunicationDevice(chosen)) selectedDevice = chosen
        } else {
            val available = manager.getDevices(AudioManager.GET_DEVICES_INPUTS)
            val bluetooth = available.any { it.type == AudioDeviceInfo.TYPE_BLUETOOTH_SCO }
            if (!bluetooth && startedSco) { manager.stopBluetoothSco(); manager.isBluetoothScoOn = false; startedSco = false }
            if (bluetooth && !startedSco && manager.isBluetoothScoAvailableOffCall) {
                manager.startBluetoothSco(); manager.isBluetoothScoOn = true; startedSco = true
            }
            manager.isSpeakerphoneOn = !bluetooth && available.none { it.type in listOf(AudioDeviceInfo.TYPE_WIRED_HEADSET, AudioDeviceInfo.TYPE_USB_HEADSET) }
        }
    }
    private fun notification(state: VoiceState, progress: String? = null): Notification {
        val snapshot = store?.state?.value
        val call = snapshot?.voice
        val bot = snapshot?.bots?.firstOrNull { it.id == call?.botId }
        val url = Uri.Builder().scheme(BuildConfig.URL_SCHEME).authority("bot").appendPath(call?.botId.orEmpty())
            .appendQueryParameter("scope", snapshot?.contextId).appendQueryParameter("computer", snapshot?.computer?.id).build()
        val content = PendingIntent.getActivity(this, NOTIFICATION, Intent(this, MainActivity::class.java)
            .setAction(Intent.ACTION_VIEW).setData(url), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        fun action(kind: String) = PendingIntent.getService(this, kind.hashCode(), Intent(this, VoiceService::class.java)
            .setAction(kind).putExtra(ID, callId), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        // An ongoing call, as Android draws one (iOS: the call's Live Activity): the bot's face and name, the call's length
        // counting up, hang up and mute here, and the status bar's call chip while it lasts.
        val face = bot?.let { notificationFace(it.avatarShape, it.avatarColor, it.customIcon) }
        val caller = Person.Builder().setName(bot?.name ?: "Codync").setImportant(true)
            .apply { face?.let { setIcon(IconCompat.createWithBitmap(it)) } }.build()
        return NotificationCompat.Builder(this, CHANNEL).setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(bot?.name ?: "Codync")
            .setContentText(callNotificationText(state, progress))
            .setStyle(NotificationCompat.CallStyle.forOngoingCall(caller, action(END)))
            // The call's length counts up beside the bot's name.
            .setUsesChronometer(true).setShowWhen(true)
            .setWhen(callStartedWallClock(session?.startedAt, SystemClock.elapsedRealtime(), System.currentTimeMillis()))
            .setContentIntent(content).setOngoing(true).setCategory(NotificationCompat.CATEGORY_CALL)
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE).setOnlyAlertOnce(true)
            .addAction(0, if (state.muted) "Unmute" else "Mute", action(MUTE)).build()
    }
    private fun end(error: String?) {
        if (ending) return
        ending = true
        val id = callId
        if (id != null) store?.finishVoice(id, error)
        val engine = session?.engine
        scope.launch { try { engine?.awaitEnd() } finally { stopSelf() } }
    }
    override fun onTaskRemoved(rootIntent: Intent?) { end(null); super.onTaskRemoved(rootIntent) }
    override fun onDestroy() {
        val id = callId
        if (id != null) store?.finishVoice(id)
        val engine = session?.engine; session = null
        if (receivers) unregisterReceiver(noisy)
        runCatching { manager.unregisterAudioDeviceCallback(devices) }
        runCatching { manager.unregisterAudioRecordingCallback(recording) }
        // A UI end or account retirement can destroy this service first. Keep
        // the route and audio focus until its recorder worker has joined.
        scope.launch {
            try { engine?.awaitEnd() }
            finally {
                runCatching { releaseRoute() }
                if (currentId == id) currentId = null
                scope.cancel()
            }
        }
        stopForeground(STOP_FOREGROUND_REMOVE); super.onDestroy()
    }
    @Suppress("DEPRECATION") private fun releaseRoute() {
        focus?.let { manager.abandonAudioFocusRequest(it) }; focus = null
        if (previousMode != null && manager.mode == AudioManager.MODE_IN_COMMUNICATION) {
            if (Build.VERSION.SDK_INT >= 31) {
                if (manager.communicationDevice?.id == selectedDevice?.id) {
                    val previous = previousDevice
                    if (previous != null && manager.availableCommunicationDevices.any { it.id == previous.id }) manager.setCommunicationDevice(previous)
                    else manager.clearCommunicationDevice()
                }
            } else { if (startedSco) { manager.stopBluetoothSco(); manager.isBluetoothScoOn = false }; manager.isSpeakerphoneOn = previousSpeaker }
            manager.mode = requireNotNull(previousMode)
        }
    }
    companion object {
        private data class Request(val id: String, val botId: String, val store: AppStore)
        private var pending: Request? = null
        private var currentId: String? = null
        private const val ID = "codync:voice"
        private const val START = "com.codync.android.voice.START"
        private const val MUTE = "com.codync.android.voice.MUTE"
        private const val END = "com.codync.android.voice.END"
        private const val CHANNEL = "codync-voice"
        private const val NOTIFICATION = 308
        internal fun start(context: Context, store: AppStore, id: String, botId: String) {
            check(pending == null && currentId == null); pending = Request(id, botId, store)
            try { ContextCompat.startForegroundService(context, Intent(context, VoiceService::class.java).setAction(START).putExtra(ID, id)) }
            catch (error: Exception) { pending = null; throw error }
        }
        internal fun stop(context: Context, id: String) {
            if (pending?.id != id && currentId != id) return
            if (pending?.id == id) pending = null
            context.stopService(Intent(context, VoiceService::class.java))
        }
    }
}

/** When the call started on the wall clock, for the notification's chronometer; `startedAt` is on `elapsedRealtime`. */
internal fun callStartedWallClock(startedAt: Long?, nowElapsed: Long, nowWall: Long): Long =
    nowWall - (nowElapsed - (startedAt ?: nowElapsed)).coerceAtLeast(0)

/** The call's notification text: its state, then what the bot is doing (the computer's latest progress line). */
internal fun callNotificationText(state: VoiceState, progress: String?, separator: String = " · "): String {
    val status = if (state.muted) "Microphone muted" else when (state.phase) {
        VoicePhase.Starting -> "Connecting…"; VoicePhase.Reconnecting -> "Reconnecting…"; VoicePhase.Speaking -> "Speaking"
        VoicePhase.Interrupted -> "Audio interrupted · return to resume"
        else -> "Listening · " + (state.provider?.title ?: "on this phone")
    }
    return if (progress.isNullOrBlank()) status else status + separator + progress
}

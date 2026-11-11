package com.codync.android

import android.content.Context
import android.os.Build
import android.os.SystemClock
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.codync.android.design.CardForm
import com.codync.android.design.CardSection
import com.codync.android.design.ChoicePicker
import com.codync.android.design.CodyncSheet
import com.codync.android.design.CodyncSwitch
import com.codync.android.design.ModalHeader
import com.codync.android.design.Palette
import com.codync.android.design.PlainTextField
import com.codync.android.design.SecondaryButton
import com.codync.android.design.SheetDetent
import com.codync.android.design.Symbol
import com.codync.android.design.Type
import com.codync.android.design.ValueRow
import com.codync.android.design.bold
import com.codync.android.design.plainButton
import com.codync.android.design.tr
import java.security.MessageDigest
import java.security.SecureRandom
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * A lock of this phone's Codync after it is left, as iOS Settings > App lock: a PIN, or the phone's
 * face, fingerprint or screen lock, or both. It only guards this device — the computer still decides
 * what any request may do. Four wrong PINs make each further try wait longer; ten sign out, so the
 * phone never holds a copy the PIN no longer protects.
 */
internal object AppLock {
    private const val PREFS = "codync-app-lock"
    private const val ON = "on"
    private const val PIN = "pinHash"
    private const val SALT = "pinSalt"
    private const val BIOMETRICS = "biometrics"
    private const val GRACE = "graceMs"
    private const val AWAY = "awayAt"
    private const val FAILURES = "failures"
    const val PIN_LENGTH = 4

    /** The lock is up and the app underneath waits. */
    var locked by mutableStateOf(false)
        private set

    fun isOn(context: Context): Boolean = prefs(context).getBoolean(ON, false) && prefs(context).getString(PIN, null) != null

    fun hasPin(context: Context): Boolean = prefs(context).getString(PIN, null) != null

    /** Whether this phone has a fingerprint, face or screen lock the gate can also accept. */
    fun biometricsAvailable(context: Context): Boolean = SensitiveAccess.available(context)

    fun biometrics(context: Context): Boolean = prefs(context).getBoolean(BIOMETRICS, true) && biometricsAvailable(context)
    fun setBiometrics(context: Context, on: Boolean) = prefs(context).edit().putBoolean(BIOMETRICS, on).apply()

    /** How long the app may be away before it locks again. */
    fun grace(context: Context): Int = prefs(context).getInt(GRACE, 0)
    fun setGrace(context: Context, millis: Int) = prefs(context).edit().putInt(GRACE, millis).apply()

    const val MIN_PIN = 4
    const val MAX_PIN = 6

    /** Locks at the next start; also turns the lock on when it was off. */
    fun turnOn(context: Context, pin: String) {
        setPin(context, pin)
        prefs(context).edit().putBoolean(ON, true).putInt(FAILURES, 0).apply()
        lock()
    }

    fun turnOff(context: Context) {
        prefs(context).edit().putBoolean(ON, false).putInt(FAILURES, 0).apply()
        locked = false
    }

    fun setPin(context: Context, pin: String) {
        val salt = ByteArray(16).also { SecureRandom().nextBytes(it) }
        val hash = hash(salt, pin)
        prefs(context).edit().putString(SALT, salt.toHex()).putString(PIN, hash.toHex()).putInt(FAILURES, 0).apply()
    }

    /** How many seconds the next attempt waits (0 when it may go now). */
    fun waitSeconds(context: Context): Int {
        val failures = prefs(context).getInt(FAILURES, 0)
        if (failures < 4) return 0
        val wait = minOf(60, 1 shl (failures - 4)) * 1_000L
        val since = SystemClock.elapsedRealtime() - prefs(context).getLong("lastTry", 0L)
        return maxOf(0L, wait - since).let { ((it + 999) / 1_000).toInt() }
    }

    /** True when the PIN matches; a wrong one starts (or extends) the wait. */
    fun check(context: Context, pin: String): Boolean {
        val preferences = prefs(context)
        val salt = preferences.getString(SALT, null)?.hex() ?: return false
        val expected = preferences.getString(PIN, null) ?: return false
        if (hash(salt, pin).toHex() != expected) {
            preferences.edit().putInt(FAILURES, preferences.getInt(FAILURES, 0) + 1)
                .putLong("lastTry", SystemClock.elapsedRealtime()).apply()
            return false
        }
        preferences.edit().putInt(FAILURES, 0).apply()
        locked = false
        unlockedAt = SystemClock.elapsedRealtime()
        SensitiveAccess.revoke()
        return true
    }

    fun failures(context: Context): Int = prefs(context).getInt(FAILURES, 0)

    fun lock() { locked = true }
    fun unlock() { locked = false; unlockedAt = SystemClock.elapsedRealtime() }
    private var unlockedAt = 0L

    /** The app went off screen: note when, so a quick return may skip the lock. */
    fun left(context: Context) {
        if (isOn(context)) prefs(context).edit().putLong(AWAY, SystemClock.elapsedRealtime()).apply()
    }

    /** Back on screen: lock again once the grace is over. A prompt that closed a moment ago just unlocked it. */
    fun returned(context: Context) {
        if (!isOn(context)) return
        if (!locked && SystemClock.elapsedRealtime() - unlockedAt < 2_000) return
        val away = SystemClock.elapsedRealtime() - prefs(context).getLong(AWAY, 0L)
        if (away >= grace(context)) lock()
    }

    /** A cold start of a locked app always asks. */
    fun started(context: Context) {
        if (isOn(context)) lock()
    }

    private fun hash(salt: ByteArray, pin: String): ByteArray =
        MessageDigest.getInstance("SHA-256").apply { update(salt); update(pin.toByteArray()) }.digest()

    private fun ByteArray.toHex(): String = joinToString("") { "%02x".format(it) }
    private fun String.hex(): ByteArray = chunked(2).map { it.toInt(16).toByte() }.toByteArray()

    private fun prefs(context: Context) = context.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
}

/** The full-screen lock: the PIN pad, and the face or fingerprint when there is one. Nothing behind it is reachable. */
@Composable internal fun AppLockGate(store: AppStore) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var pin by remember { mutableStateOf("") }
    var wrong by remember { mutableStateOf(false) }
    var wait by remember { mutableIntStateOf(AppLock.waitSeconds(context)) }
    val focus = remember { FocusRequester() }
    val biometrics = remember { AppLock.biometrics(context) }
    val signingOut = remember { mutableStateOf(false) }

    fun finish() { pin = ""; wrong = false }

    fun submit() {
        if (pin.length < AppLock.MIN_PIN || wait > 0) return
        if (AppLock.check(context, pin)) finish()
        else {
            wrong = true
            pin = ""
            wait = AppLock.waitSeconds(context)
            if (AppLock.failures(context) >= 10 && !signingOut.value) {
                signingOut.value = true
                store.signOut()
            }
        }
    }

    val prompt = {
        scope.launch {
            if (SensitiveAccess.ask(context, "Unlock Codync")) { AppLock.unlock(); finish() }
        }
    }

    BackHandler(enabled = true) {}
    LaunchedEffect(Unit) { if (!biometrics) focus.requestFocus() }
    LaunchedEffect(wait) {
        while (wait > 0) {
            delay(1_000)
            wait = AppLock.waitSeconds(context)
        }
    }

    Box(Modifier.fillMaxSize().background(Palette.background).safeDrawingPadding().padding(24.dp), contentAlignment = Alignment.Center) {
        Column(Modifier.widthIn(max = 360.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(14.dp)) {
            Box(Modifier.clip(CircleShape).background(Palette.surface).padding(20.dp)) {
                Symbol("lock.fill", size = 26.dp, tint = Palette.secondary)
            }
            Text(tr("Codync is locked"), style = Type.title3.bold, color = Palette.text)
            Text(
                when {
                    signingOut.value -> tr("Signed out after ten wrong PINs.")
                    wait > 0 -> tr("Too many tries. Wait {count} s, or use your phone's unlock.").replace("{count}", wait.toString())
                    wrong -> tr("Wrong PIN. Try again.")
                    else -> tr("Enter your PIN to get back to your agents.")
                },
                style = Type.subheadline, color = Palette.secondary, textAlign = TextAlign.Center,
            )
            PlainTextField(pin, { if (it.length <= AppLock.MAX_PIN && it.all(Char::isDigit)) pin = it },
                Modifier.fillMaxWidth().focusRequester(focus).clip(CircleShape).background(Palette.surface).padding(horizontal = 16.dp, vertical = 12.dp),
                prompt = tr("PIN"), singleLine = true, style = Type.body.copy(textAlign = TextAlign.Center),
                keyboard = KeyboardOptions(keyboardType = KeyboardType.NumberPassword), enabled = wait == 0 && !signingOut.value)
            SecondaryButton(tr("Unlock"), Modifier.fillMaxWidth(), enabled = pin.length >= AppLock.MIN_PIN && wait == 0) { submit() }
            if (biometrics) SecondaryButton(tr("Use face or fingerprint"), Modifier.fillMaxWidth()) { prompt() }
            if (AppLock.isOn(context) && !signingOut.value) SecondaryButton(tr("Sign out instead"), Modifier.fillMaxWidth()) { store.signOut() }
        }
    }
}

/** Settings > App lock: the switch, the PIN, the phone unlock, and how long a trip away may last. */
@Composable internal fun AppLockSection() {
    val context = LocalContext.current
    var on by remember { mutableStateOf(AppLock.isOn(context)) }
    var changingPin by remember { mutableStateOf(false) }
    var grace by remember { mutableIntStateOf(AppLock.grace(context)) }
    var usePhone by remember { mutableStateOf(AppLock.biometrics(context)) }
    val available = remember { AppLock.biometricsAvailable(context) }
    val graces = listOf(0 to tr("Immediately"), 30_000 to tr("After 30 seconds"), 60_000 to tr("After a minute"),
        300_000 to tr("After 5 minutes"), 3_600_000 to tr("After an hour"))

    CardSection(
        title = tr("App lock"),
        footer = if (available) tr("Ask for a PIN, your fingerprint, face or screen lock when Codync starts and after you leave it. The PIN still works when the phone's unlock doesn't.")
        else tr("Ask for a PIN when Codync starts and after you leave it. Enroll a fingerprint or face in Android settings to unlock with one too."),
    ) {
        row { SwitchRow(tr("Lock Codync"), isOn = on, onChange = { want ->
            if (want) changingPin = true else { on = false; AppLock.turnOff(context) }
        }) }
        if (on) {
            row { ValueRow(tr("PIN"), if (AppLock.hasPin(context)) tr("{count} digits").replace("{count}", AppLock.PIN_LENGTH.toString()) else tr("Set one"), Modifier.plainButton { changingPin = true }) }
            row { SwitchRow(tr("Unlock with face or fingerprint"), isOn = usePhone, enabled = available,
                onChange = { usePhone = it; AppLock.setBiometrics(context, it) }) }
            row {
                ValueRow(tr("Lock after")) {
                    ChoicePicker(grace, graces, { grace = it; AppLock.setGrace(context, it) }, label = tr("Lock after"))
                }
            }
        }
    }

    if (changingPin) PinSheet({ pin ->
        AppLock.turnOn(context, pin)
        on = true
        changingPin = false
    }, { changingPin = false })
}

/** A new PIN, typed twice; four to six digits. */
@Composable private fun PinSheet(save: (String) -> Unit, dismiss: () -> Unit) {
    var pin by remember { mutableStateOf("") }
    var again by remember { mutableStateOf("") }
    val focus = remember { FocusRequester() }
    val ready = pin.length >= AppLock.MIN_PIN && pin == again
    fun digits(value: String) = value.filter(Char::isDigit).take(AppLock.MAX_PIN)

    CodyncSheet(true, dismiss, detents = setOf(SheetDetent.Medium)) {
        Column(Modifier.fillMaxWidth().background(Palette.background)) {
            ModalHeader(tr("App lock PIN"))
            CardForm(padding = androidx.compose.foundation.layout.PaddingValues(20.dp)) {
                Text(tr("Four to six digits. After ten wrong tries Codync signs out on this phone."),
                    style = Type.footnote, color = Palette.secondary)
                PlainTextField(pin, { pin = digits(it) }, Modifier.fillMaxWidth().focusRequester(focus), prompt = tr("PIN"), singleLine = true,
                    keyboard = KeyboardOptions(keyboardType = KeyboardType.NumberPassword))
                PlainTextField(again, { again = digits(it) }, Modifier.fillMaxWidth(), prompt = tr("PIN again"), singleLine = true,
                    keyboard = KeyboardOptions(keyboardType = KeyboardType.NumberPassword))
                if (again.isNotEmpty() && pin != again) Text(tr("The two PINs differ."), style = Type.footnote, color = Palette.danger)
                SecondaryButton(tr("Save PIN"), Modifier.fillMaxWidth(), enabled = ready) { save(pin) }
            }
        }
        LaunchedEffect(Unit) { if (Build.VERSION.SDK_INT >= 23) focus.requestFocus() }
    }
}

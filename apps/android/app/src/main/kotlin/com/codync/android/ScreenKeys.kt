package com.codync.android

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.text.Editable
import android.text.InputType
import android.text.TextWatcher
import android.util.TypedValue
import android.view.KeyEvent
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputConnectionWrapper
import android.view.inputmethod.InputMethodManager
import android.widget.EditText

/**
 * Invisible text field that turns the soft keyboard (including IME input such as Zhuyin or
 * Pinyin) and hardware keys into events for the computer (CodyncUI `KeyProxyField`). It keeps
 * focus with the soft keyboard hidden, so a hardware keyboard just works.
 */
@SuppressLint("AppCompatCustomView", "ViewConstructor")
internal class KeyProxyField(context: Context) : EditText(context) {
    var onText: (String) -> Unit = {}
    var onKey: (String, List<String>) -> Unit = { _, _ -> }
    /** The composition started or ended: the canvas shows the field while an IME composes. */
    var onComposingChanged: (Boolean) -> Unit = {}
    private var composing = false

    var showsSoftKeyboard = false
        set(value) {
            if (field == value) return
            field = value
            requestFocus()
            val keyboard = context.getSystemService(InputMethodManager::class.java)
            if (value) keyboard.showSoftInput(this, 0) else keyboard.hideSoftInputFromWindow(windowToken, 0)
        }

    init {
        inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS or InputType.TYPE_TEXT_FLAG_MULTI_LINE
        imeOptions = EditorInfo.IME_FLAG_NO_EXTRACT_UI or EditorInfo.IME_FLAG_NO_FULLSCREEN or EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING
        showSoftInputOnFocus = false
        isFocusableInTouchMode = true
        alpha = IDLE_ALPHA
        setTextColor(Color.WHITE)
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 17f)
        val dp = resources.displayMetrics.density
        background = GradientDrawable().apply { setColor(Color.rgb(31, 31, 31)); cornerRadius = 12 * dp }
        setPadding((14 * dp).toInt(), (8 * dp).toInt(), (14 * dp).toInt(), (8 * dp).toInt())
        addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence, start: Int, count: Int, after: Int) = Unit
            override fun onTextChanged(s: CharSequence, start: Int, before: Int, count: Int) = Unit
            override fun afterTextChanged(s: Editable) = changed(s)
        })
        setOnKeyListener { _, _, event -> key(event) }
    }

    /** Sends committed text; text still being composed by an IME waits. */
    private fun changed(text: Editable) {
        val now = BaseInputConnection.getComposingSpanStart(text) != -1
        if (now != composing) {
            composing = now
            alpha = if (now) 1f else IDLE_ALPHA
            onComposingChanged(now)
        }
        if (now || text.isEmpty()) return
        val committed = text.toString()
        text.clear()
        // Return is its own key; the multi-line field would type it as a newline.
        committed.split('\n').forEachIndexed { i, part ->
            if (i > 0) onKey("return", emptyList())
            if (part.isNotEmpty()) onText(part)
        }
    }

    /** While an IME is composing, Return, Delete, arrows and Escape pick and edit candidates. */
    private fun key(event: KeyEvent): Boolean {
        if (composing) return false
        val (name, mods) = ScreenKeys.map(event.keyCode, event.metaState, event.getUnicodeChar(0)) ?: return false
        if (event.action == KeyEvent.ACTION_DOWN) onKey(name, mods)
        return true
    }

    /** Backspace on an empty field still deletes on the computer, however the IME sends it. */
    override fun onCreateInputConnection(outAttrs: EditorInfo): InputConnection? {
        val base = super.onCreateInputConnection(outAttrs) ?: return null
        return object : InputConnectionWrapper(base, true) {
            override fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean {
                if (text.isNullOrEmpty() && beforeLength > 0) { onKey("delete", emptyList()); return true }
                return super.deleteSurroundingText(beforeLength, afterLength)
            }
        }
    }

    private companion object { const val IDLE_ALPHA = 0.02f }
}

/** Hardware keys as the computer names them. Shortcuts and special keys go straight through; plain typing takes the text path (IME-aware). */
internal object ScreenKeys {
    private val special = mapOf(
        KeyEvent.KEYCODE_DPAD_UP to "up", KeyEvent.KEYCODE_DPAD_DOWN to "down", KeyEvent.KEYCODE_DPAD_LEFT to "left", KeyEvent.KEYCODE_DPAD_RIGHT to "right",
        KeyEvent.KEYCODE_ESCAPE to "escape", KeyEvent.KEYCODE_TAB to "tab", KeyEvent.KEYCODE_ENTER to "return", KeyEvent.KEYCODE_NUMPAD_ENTER to "return",
        KeyEvent.KEYCODE_DEL to "delete", KeyEvent.KEYCODE_FORWARD_DEL to "forwardDelete",
        KeyEvent.KEYCODE_MOVE_HOME to "home", KeyEvent.KEYCODE_MOVE_END to "end", KeyEvent.KEYCODE_PAGE_UP to "pageUp", KeyEvent.KEYCODE_PAGE_DOWN to "pageDown",
    ) + (1..12).associate { KeyEvent.KEYCODE_F1 + it - 1 to "f$it" }

    fun modifiers(meta: Int): List<String> = buildList {
        if (meta and KeyEvent.META_META_ON != 0) add("cmd")
        if (meta and KeyEvent.META_ALT_ON != 0) add("option")
        if (meta and KeyEvent.META_CTRL_ON != 0) add("ctrl")
        if (meta and KeyEvent.META_SHIFT_ON != 0) add("shift")
    }

    /** `char` is the key's character without modifiers (0: none). `null`: let the field type it. */
    fun map(keyCode: Int, meta: Int, char: Int): Pair<String, List<String>>? {
        val mods = modifiers(meta)
        special[keyCode]?.let { return it to mods }
        // Ctrl-Space switches the keyboard's input language.
        if (keyCode == KeyEvent.KEYCODE_SPACE && mods == listOf("ctrl")) return null
        val chord = "cmd" in mods || "ctrl" in mods || "option" in mods
        if (!chord || char <= 0) return null
        return String(Character.toChars(char)).lowercase() to mods
    }
}

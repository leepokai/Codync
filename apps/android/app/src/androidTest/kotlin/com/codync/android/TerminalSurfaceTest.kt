package com.codync.android

import android.view.KeyEvent
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.unit.dp
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import kotlin.coroutines.resume

class TerminalSurfaceTest {
    @get:Rule val compose = createComposeRule()
    private val ready = AtomicReference<TerminalRenderer>()
    private val created = AtomicReference<TerminalRenderer>()
    private val failures = AtomicInteger()
    private val inputs = CopyOnWriteArrayList<ByteArray>()
    private val sizes = CopyOnWriteArrayList<Pair<Int, Int>>()

    private fun show(height: State<Int> = mutableStateOf(400), visible: State<Boolean> = mutableStateOf(true)): TerminalRenderer {
        compose.setContent {
            if (visible.value) TerminalSurface(Modifier.fillMaxWidth().height(height.value.dp), ready::set,
                inputs::add, { cols, rows -> sizes.add(cols to rows) }, { failures.incrementAndGet() }, created = created::set)
        }
        compose.waitUntil(15_000) { ready.get() != null || failures.get() != 0 }
        if (failures.get() != 0) fail("Bundled terminal must load: " + script(requireNotNull(created.get()),
            "JSON.stringify({viewport:[innerWidth,innerHeight],main:document.getElementById('terminal').getBoundingClientRect().toJSON(),body:getComputedStyle(document.body).height,html:getComputedStyle(document.documentElement).height,mainPosition:getComputedStyle(document.getElementById('terminal')).position,sheets:Array.from(document.styleSheets,s=>({href:s.href,rules:s.cssRules.length}))})"))
        return requireNotNull(ready.get())
    }

    private fun script(renderer: TerminalRenderer, source: String): JsonElement = runBlocking {
        withContext(Dispatchers.Main) {
            withTimeout(10_000) {
                suspendCancellableCoroutine { reply -> renderer.view.evaluateJavascript(source) { value ->
                    if (reply.isActive) reply.resume(Json.parseToJsonElement(value))
                } }
            }
        }
    }

    private fun write(renderer: TerminalRenderer, bytes: ByteArray) = runBlocking {
        withContext(Dispatchers.Main) { renderer.write(bytes) }
    }
    private fun editor(renderer: TerminalRenderer): android.view.inputmethod.InputConnection {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        compose.waitUntil(5_000) { renderer.view.hasWindowFocus() }
        instrumentation.waitForIdleSync()
        android.os.SystemClock.sleep(500) // Native IME attach/restart is asynchronous after WebView focus.
        val connection = AtomicReference<android.view.inputmethod.InputConnection>()
        compose.waitUntil(5_000) {
            compose.runOnUiThread {
                val info = android.view.inputmethod.EditorInfo()
                val candidate = renderer.view.onCreateInputConnection(info)
                if (candidate != null && info.inputType != android.text.InputType.TYPE_NULL) connection.set(candidate)
            }
            connection.get() != null
        }
        return requireNotNull(connection.get())
    }

    @Test fun ansiAndSplitUtf8RenderAsTextWithoutExecutingOutput() {
        val renderer = show()
        write(renderer, "before\r\u001b[2K\u001b[31mred\u001b[0m\r\n".toByteArray())
        val unicode = "café 🐈\r\n".toByteArray()
        write(renderer, unicode.copyOfRange(0, 4))
        write(renderer, unicode.copyOfRange(4, 8))
        write(renderer, unicode.copyOfRange(8, unicode.size))
        write(renderer, "<script>window.outputExecuted=true</script>".toByteArray())
        val text = script(renderer, "codyncTerminal.text()").jsonPrimitive.content
        assertTrue(text.contains("red\ncafé 🐈"))
        assertFalse(text.contains("before"))
        assertTrue(text.contains("<script>window.outputExecuted=true</script>"))
        assertEquals("undefined", script(renderer, "typeof window.outputExecuted").jsonPrimitive.content)
        assertEquals(0, failures.get())
    }

    @Test fun hardwareKeysReachTheHostInOrderAndTheTerminalFitsAfterResize() {
        val height = mutableStateOf(400)
        val renderer = show(height)
        compose.waitUntil(5_000) { renderer.view.hasWindowFocus() }
        compose.runOnIdle { renderer.focus() }
        script(renderer, "window.fixtureKeys = []; for (const type of ['keydown', 'keyup', 'input']) document.addEventListener(type, e => window.fixtureKeys.push([type,e.key,e.keyCode,e.inputType,e.target.value]), true)")
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        compose.runOnUiThread {
            renderer.view.context.getSystemService(android.view.inputmethod.InputMethodManager::class.java)
                .hideSoftInputFromWindow(renderer.view.windowToken, 0)
        }
        instrumentation.waitForIdleSync()
        android.os.SystemClock.sleep(500) // Wait for the IME hide animation before injecting physical keys.
        val keyboard = android.view.InputDevice.getDeviceIds().toList().mapNotNull(android.view.InputDevice::getDevice)
            .firstOrNull { !it.isVirtual && it.keyboardType == android.view.InputDevice.KEYBOARD_TYPE_ALPHABETIC &&
                it.keyCharacterMap.get(KeyEvent.KEYCODE_A, 0) == 'a'.code }
        assertNotNull("The emulator needs an alphabetic hardware keyboard for this gate", keyboard)
        for ((key, scan) in listOf(KeyEvent.KEYCODE_A to 30, KeyEvent.KEYCODE_B to 48, KeyEvent.KEYCODE_C to 46, KeyEvent.KEYCODE_ENTER to 28)) {
            val time = android.os.SystemClock.uptimeMillis()
            val down = KeyEvent(time, time, KeyEvent.ACTION_DOWN, key, 0, 0, requireNotNull(keyboard).id, scan, KeyEvent.FLAG_FROM_SYSTEM, android.view.InputDevice.SOURCE_KEYBOARD)
            instrumentation.sendKeySync(down); instrumentation.sendKeySync(KeyEvent.changeAction(down, KeyEvent.ACTION_UP))
            instrumentation.waitForIdleSync()
            android.os.SystemClock.sleep(100)
        }
        repeat(100) { if (inputs.sumOf { it.size } < 4) android.os.SystemClock.sleep(50) }
        if (inputs.sumOf { it.size } < 4) fail("Fixture keyboard events: " + script(renderer, "[window.fixtureKeys, document.activeElement.tagName,document.hasFocus(),document.visibilityState]") + "; received: " + inputs.flatMap { it.toList() }.toByteArray().toString(Charsets.UTF_8) + "; native focus: " + renderer.view.hasFocus() + "; window focus: " + renderer.view.hasWindowFocus())
        assertEquals("Fixture keyboard events: " + script(renderer, "window.fixtureKeys"), "abc\r", inputs.flatMap { it.toList() }.toByteArray().toString(Charsets.UTF_8))
        compose.waitUntil(5_000) { sizes.isNotEmpty() }
        val originalRows = sizes.last().second
        compose.runOnIdle { height.value = 200 }
        compose.waitUntil(5_000) { sizes.last().second < originalRows }
        assertTrue(sizes.last().first >= 20)
        assertEquals(0, failures.get())
    }

    @Test fun imeCommitSendsUnicodeAndPastedTextAsUtf8() {
        val renderer = show()
        compose.waitUntil(5_000) { renderer.view.hasWindowFocus() }
        compose.runOnIdle {
            renderer.focus(showKeyboard = true)
        }
        script(renderer, "codyncTerminal.focus()")
        val active = editor(renderer)
        val committed = CompletableDeferred<Boolean>()
        compose.runOnIdle {
            val handler = active.handler ?: android.os.Handler(android.os.Looper.getMainLooper())
            handler.post { committed.complete(active.commitText("café 🐈", 1)) }
        }
        val delivered = runBlocking { withTimeout(5_000) { committed.await() } }
        assertTrue(delivered)
        repeat(100) { if (inputs.sumOf { it.size } < "café 🐈".toByteArray().size) android.os.SystemClock.sleep(50) }
        if (inputs.sumOf { it.size } < "café 🐈".toByteArray().size) fail("Fixture IME state: " + script(renderer,
            "[document.activeElement.value,document.hasFocus(),document.visibilityState]") + "; connection: " + active.javaClass.name + "; received: " + inputs.sumOf { it.size })
        assertEquals("café 🐈", inputs.flatMap { it.toList() }.toByteArray().toString(Charsets.UTF_8))
    }

    @Test fun imeCommitThenEnterDoesNotSendADuplicateCharacterOrBackspace() {
        val renderer = show()
        compose.waitUntil(5_000) { renderer.view.hasWindowFocus() }
        compose.runOnIdle { renderer.focus(showKeyboard = true) }
        script(renderer, "codyncTerminal.focus()")
        val active = editor(renderer)
        script(renderer, "window.fixtureKeys = []; for (const type of ['keydown', 'keyup', 'input', 'compositionend']) document.addEventListener(type, e => window.fixtureKeys.push([type,e.key,e.keyCode,e.inputType,e.data,e.target.value]), true)")
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        for (text in listOf("café 🐈", "next")) {
            val committed = CompletableDeferred<Boolean>()
            compose.runOnUiThread {
                val handler = active.handler ?: android.os.Handler(android.os.Looper.getMainLooper())
                handler.post { committed.complete(active.commitText(text, 1)) }
            }
            val delivered = runBlocking { withTimeout(5_000) { committed.await() } }
            assertTrue(delivered)
            script(renderer, "true")
            instrumentation.sendKeyDownUpSync(KeyEvent.KEYCODE_ENTER)
            instrumentation.waitForIdleSync()
            android.os.SystemClock.sleep(200)
        }
        android.os.SystemClock.sleep(1_000)
        assertEquals("Fixture input events: " + script(renderer, "window.fixtureKeys"), "café 🐈\rnext\r", inputs.flatMap { it.toList() }.toByteArray().toString(Charsets.UTF_8))
    }

    @Test fun anImeCompositionSendsOnlyItsFinalUnicodeText() {
        val renderer = show()
        compose.waitUntil(5_000) { renderer.view.hasWindowFocus() }
        compose.runOnIdle { renderer.focus(showKeyboard = true) }
        script(renderer, "codyncTerminal.focus()")
        val active = editor(renderer)
        val composed = CompletableDeferred<Boolean>()
        script(renderer, "window.fixtureKeys = []; for (const type of ['keydown', 'keyup', 'input', 'compositionstart', 'compositionupdate', 'compositionend']) document.addEventListener(type, e => window.fixtureKeys.push([type,e.key,e.keyCode,e.inputType,e.data,e.target.value]), true)")
        compose.runOnUiThread {
            val handler = active.handler ?: android.os.Handler(android.os.Looper.getMainLooper())
            handler.post { composed.complete(active.setComposingText("cafe", 1) &&
                active.setComposingText("café 🐈", 1) && active.finishComposingText()) }
        }
        val delivered = runBlocking { withTimeout(5_000) { composed.await() } }
        assertTrue(delivered)
        compose.waitUntil(5_000) { inputs.sumOf { it.size } >= "café 🐈".toByteArray().size }
        assertEquals("café 🐈", inputs.flatMap { it.toList() }.toByteArray().toString(Charsets.UTF_8))
        // The JS bridge reports committed bytes before WebView has necessarily
        // acknowledged its cleared editor to Android's IME. Await that boundary
        // before mixing this synthetic composition with a hardware delete.
        runBlocking { withTimeout(5_000) {
            while (true) {
                val empty = CompletableDeferred<Boolean>()
                compose.runOnUiThread {
                    val handler = active.handler ?: android.os.Handler(android.os.Looper.getMainLooper())
                    handler.post { empty.complete(active.getTextBeforeCursor(256, 0)?.isEmpty() == true &&
                        active.getTextAfterCursor(256, 0)?.isEmpty() == true) }
                }
                val acknowledged = empty.await()
                if (acknowledged) break
                kotlinx.coroutines.delay(50)
            }
        } }
        InstrumentationRegistry.getInstrumentation().sendKeyDownUpSync(KeyEvent.KEYCODE_DEL)
        android.os.SystemClock.sleep(1_000)
        assertEquals("Fixture input events: " + script(renderer, "window.fixtureKeys"), "café 🐈\u007f", inputs.flatMap { it.toList() }.toByteArray().toString(Charsets.UTF_8))
    }

    @Test fun leavingTheSurfaceCancelsAnUnacknowledgedWrite() {
        val visible = mutableStateOf(true)
        val renderer = show(visible = visible)
        script(renderer, "codyncTerminal.write = function() {}")
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
        val write = scope.async { renderer.write("pending".toByteArray()) }
        script(renderer, "true") // Drain the posted write before removing its view.
        compose.runOnIdle { visible.value = false }
        compose.waitUntil { renderer.disposed }
        runBlocking {
            try { write.await(); fail("A removed renderer must cancel its waiting writer") }
            catch (_: CancellationException) { }
        }
        scope.cancel()
    }

    @Test fun onlyHttpLinksOpenAndOutputCannotWriteTheClipboardByItself() {
        val links = CopyOnWriteArrayList<String>(); val copies = CopyOnWriteArrayList<String>()
        compose.setContent { TerminalSurface(Modifier.fillMaxWidth().height(400.dp), ready::set, {}, { _, _ -> },
            { failures.incrementAndGet() }, { links.add(it.toString()) }, copies::add) }
        compose.waitUntil(15_000) { ready.get() != null || failures.get() != 0 }
        val renderer = requireNotNull(ready.get())
        script(renderer, "CodyncTerminal.openLink('javascript:alert(1)'); CodyncTerminal.openLink('file:///private'); CodyncTerminal.openLink('https://user:pass@example.com/'); CodyncTerminal.openLink('https://example.com/setup')")
        compose.waitUntil { links.isNotEmpty() }
        assertEquals(listOf("https://example.com/setup"), links)
        write(renderer, "\u001b]52;c;ZmFrZQ==\u0007".toByteArray())
        assertTrue(copies.isEmpty())
    }

    @Test fun aTerminatedRendererReleasesItsViewWithoutTerminatingTheActivity() {
        org.junit.Assume.assumeTrue(android.os.Build.HARDWARE in listOf("ranchu", "goldfish"))
        var visible by mutableStateOf(true)
        compose.setContent {
            if (visible) TerminalSurface(Modifier.fillMaxWidth().height(400.dp), ready::set, {}, { _, _ -> }, {
                failures.incrementAndGet(); visible = false
            }) else androidx.compose.material3.Text("Terminal failed safely")
        }
        compose.waitUntil(15_000) { ready.get() != null || failures.get() != 0 }
        val renderer = requireNotNull(ready.get())
        compose.runOnIdle { assertTrue(requireNotNull(renderer.view.webViewRenderProcess).terminate()) }
        compose.waitUntil(5_000) { failures.get() == 1 }
        compose.onNodeWithText("Terminal failed safely").assertIsDisplayed()
        assertTrue(renderer.disposed)
    }
}

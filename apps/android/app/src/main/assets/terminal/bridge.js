'use strict';
(() => {
    const terminal = new Terminal({fontSize: 13, scrollback: 2000, screenReaderMode: CodyncTerminal.screenReaderEnabled(),
        theme: {background: '#111111', foreground: '#eeeeee'}, allowProposedApi: false,
        linkHandler: {activate: (_event, uri) => CodyncTerminal.openLink(uri)}});
    const fit = new FitAddon.FitAddon();
    terminal.loadAddon(fit);
    terminal.loadAddon(new WebLinksAddon.WebLinksAddon((_event, uri) => CodyncTerminal.openLink(uri)));
    terminal.open(document.getElementById('terminal'));
    // Modern Android WebView commits text after key 229 without a composition.
    // Xterm's delayed textarea diff can race Enter's clear and emit a false DEL.
    // Handle committed input through its public API; actual compositions and
    // accessibility continue through xterm's composition/screen-reader path.
    let composing = false;
    let imeKey = false;
    const editor = terminal.textarea;
    editor.addEventListener('compositionstart', () => { composing = true; imeKey = false; }, true);
    editor.addEventListener('compositionend', () => { composing = false; }, true);
    terminal.attachCustomKeyEventHandler(event => {
        if (event.type !== 'keydown') return true;
        imeKey = event.keyCode === 229 && !composing && !terminal.options.screenReaderMode;
        return !imeKey;
    });
    terminal.element.addEventListener('input', event => {
        if (event.target !== editor || !imeKey || composing || event.isComposing || terminal.options.screenReaderMode) return;
        if (event.inputType === 'insertText' && event.data) {
            event.stopImmediatePropagation();
            terminal.input(event.data, true);
            editor.value = '';
            imeKey = false;
        } else if (event.inputType === 'deleteContentBackward' || event.inputType === 'deleteContentForward') {
            event.stopImmediatePropagation();
            terminal.input(event.inputType === 'deleteContentBackward' ? '\x7f' : '\x1b[3~', true);
            editor.value = '';
            imeKey = false;
        }
    }, true);
    function encode(bytes) {
        let value = '';
        for (let i = 0; i < bytes.length; i++) value += String.fromCharCode(bytes[i]);
        return btoa(value);
    }
    let started = false;
    function fitSize() {
        const size = fit.proposeDimensions();
        // WebView can attach before xterm has measured its first cell. Retry after render.
        if (!size || size.cols < 20 || size.rows < 4) return;
        terminal.resize(Math.min(500, size.cols), Math.min(500, size.rows));
        CodyncTerminal.resized(terminal.cols, terminal.rows);
        if (!started) { started = true; CodyncTerminal.ready(); terminal.focus(); }
    }
    terminal.onData(value => {
        CodyncTerminal.input(encode(new TextEncoder().encode(value)));
        // The remote PTY owns committed text. Retaining it in the local editor
        // lets Android reopen an earlier word as a new autocorrect composition.
        if (!composing && !terminal.options.screenReaderMode) editor.value = '';
    });
    terminal.onBinary(value => {
        const bytes = Uint8Array.from(value, char => char.charCodeAt(0) & 255);
        CodyncTerminal.input(encode(bytes));
    });
    terminal.onResize(size => CodyncTerminal.resized(size.cols, size.rows));
    new ResizeObserver(fitSize).observe(document.getElementById('terminal'));
    window.codyncTerminal = {
        write(id, encoded) {
            const bytes = Uint8Array.from(atob(encoded), char => char.charCodeAt(0));
            terminal.write(bytes, () => CodyncTerminal.written(id));
        },
        reset() { terminal.reset(); },
        input(enabled) { terminal.options.disableStdin = !enabled; },
        screenReader(enabled) { terminal.options.screenReaderMode = enabled; },
        focus() { terminal.focus(); },
        copySelection() { CodyncTerminal.copy(terminal.getSelection()); },
        text() {
            let result = '';
            const buffer = terminal.buffer.active;
            for (let i = 0; i < buffer.length; i++) result += buffer.getLine(i).translateToString(true) + '\n';
            return result;
        }
    };
    let frames = 0;
    function initialFit() {
        fitSize();
        if (!started && ++frames < 600) requestAnimationFrame(initialFit);
        else if (!started) CodyncTerminal.failed();
    }
    requestAnimationFrame(initialFit);
})();

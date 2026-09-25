import AppKit
import Carbon.HIToolbox
import CoreGraphics

/// Posts mouse and keyboard events. Coordinates are display-local points.
/// Event JSON is shared by the host (bots) and the phone's data channels:
/// `move`, `down`/`up`, `click`, `drag`, `scroll`, `text`, `key` (see `host/src/screen.rs`).
@MainActor
final class InputInjector {
    private let source = CGEventSource(stateID: .hidSystemState)
    /// Button held by a phone drag (`down` … `up`).
    private var held: CGMouseButton?
    private var heldFlags: CGEventFlags = []

    func perform(_ e: [String: Any], display: CGDirectDisplayID) async throws {
        guard AXIsProcessTrusted() else {
            throw HelperError("Codync Screen isn't allowed to control this computer yet. Allow it under Privacy & Security → Accessibility.")
        }
        let origin = CGDisplayBounds(display).origin
        func point() throws -> CGPoint {
            guard let x = Self.number(e["x"]), let y = Self.number(e["y"]) else { throw HelperError("x and y are required") }
            return CGPoint(x: origin.x + x, y: origin.y + y)
        }
        let button = Self.button(e["button"])
        let flags = Self.flags(e["modifiers"])
        switch e["type"] as? String {
        case "move":
            let p = try point()
            if let held {
                post(mouse: Self.dragType(held), at: p, button: held, flags: heldFlags)
            } else {
                post(mouse: .mouseMoved, at: p, button: .left, flags: [])
            }
        case "down":
            let p = try point()
            held = button
            heldFlags = flags
            post(mouse: Self.downType(button), at: p, button: button, flags: flags, clicks: Int64(Self.number(e["count"]) ?? 1))
        case "up":
            let p = try point()
            post(mouse: Self.upType(held ?? button), at: p, button: held ?? button, flags: heldFlags, clicks: Int64(Self.number(e["count"]) ?? 1))
            held = nil
            heldFlags = []
        case "click":
            let p = try point()
            post(mouse: .mouseMoved, at: p, button: .left, flags: [])
            let count = max(1, min(3, Int(Self.number(e["count"]) ?? 1)))
            for i in 1...count {
                post(mouse: Self.downType(button), at: p, button: button, flags: flags, clicks: Int64(i))
                post(mouse: Self.upType(button), at: p, button: button, flags: flags, clicks: Int64(i))
            }
        case "drag":
            let from = try point()
            guard let tx = Self.number(e["toX"]), let ty = Self.number(e["toY"]) else { throw HelperError("toX and toY are required") }
            let to = CGPoint(x: origin.x + tx, y: origin.y + ty)
            post(mouse: .mouseMoved, at: from, button: .left, flags: [])
            post(mouse: .leftMouseDown, at: from, button: .left, flags: [])
            // Apps need intermediate moves (and a little time) to recognize a drag.
            let steps = 20
            for i in 1...steps {
                try await Task.sleep(for: .milliseconds(15))
                let t = Double(i) / Double(steps)
                post(mouse: .leftMouseDragged, at: CGPoint(x: from.x + (to.x - from.x) * t, y: from.y + (to.y - from.y) * t), button: .left, flags: [])
            }
            post(mouse: .leftMouseUp, at: to, button: .left, flags: [])
        case "scroll":
            let p = try point()
            post(mouse: .mouseMoved, at: p, button: .left, flags: [])
            let pixels = (e["units"] as? String) == "pixel"
            let dy = Int32((Self.number(e["dy"]) ?? 0).rounded()), dx = Int32((Self.number(e["dx"]) ?? 0).rounded())
            // Positive dy scrolls content down, which is a negative wheel delta.
            CGEvent(scrollWheelEvent2Source: source, units: pixels ? .pixel : .line, wheelCount: 2, wheel1: -dy, wheel2: -dx, wheel3: 0)?
                .post(tap: .cghidEventTap)
        case "text":
            type(e["text"] as? String ?? "")
        case "key":
            guard let name = e["key"] as? String, let code = Self.keyCode(name) else {
                throw HelperError("unknown key \(e["key"] ?? "")")
            }
            for down in [true, false] {
                let ev = CGEvent(keyboardEventSource: source, virtualKey: code, keyDown: down)
                ev?.flags = flags
                ev?.post(tap: .cghidEventTap)
            }
        default:
            throw HelperError("unknown input \(e["type"] ?? "")")
        }
    }

    private func post(mouse type: CGEventType, at p: CGPoint, button: CGMouseButton, flags: CGEventFlags, clicks: Int64 = 1) {
        guard let ev = CGEvent(mouseEventSource: source, mouseType: type, mouseCursorPosition: p, mouseButton: button) else { return }
        ev.flags = flags
        if type != .mouseMoved { ev.setIntegerValueField(.mouseEventClickState, value: clicks) }
        ev.post(tap: .cghidEventTap)
    }

    /// Any Unicode, independent of the keyboard layout.
    private func type(_ text: String) {
        let units = Array(text.utf16)
        // The unicode payload of one key event is capped at 20 UTF-16 units.
        for start in stride(from: 0, to: units.count, by: 16) {
            let chunk = Array(units[start..<min(start + 16, units.count)])
            for down in [true, false] {
                let ev = CGEvent(keyboardEventSource: source, virtualKey: 0, keyDown: down)
                chunk.withUnsafeBufferPointer { ev?.keyboardSetUnicodeString(stringLength: chunk.count, unicodeString: $0.baseAddress) }
                ev?.post(tap: .cghidEventTap)
            }
        }
    }

    // MARK: mapping

    static func number(_ v: Any?) -> Double? { (v as? NSNumber)?.doubleValue }

    static func button(_ v: Any?) -> CGMouseButton {
        switch v as? String {
        case "right": .right
        case "middle": .center
        default: .left
        }
    }

    static func flags(_ v: Any?) -> CGEventFlags {
        var f: CGEventFlags = []
        for m in v as? [String] ?? [] {
            switch m {
            case "cmd": f.insert(.maskCommand)
            case "option": f.insert(.maskAlternate)
            case "ctrl": f.insert(.maskControl)
            case "shift": f.insert(.maskShift)
            default: break
            }
        }
        return f
    }

    static func downType(_ b: CGMouseButton) -> CGEventType {
        switch b {
        case .left: .leftMouseDown
        case .right: .rightMouseDown
        default: .otherMouseDown
        }
    }

    static func upType(_ b: CGMouseButton) -> CGEventType {
        switch b {
        case .left: .leftMouseUp
        case .right: .rightMouseUp
        default: .otherMouseUp
        }
    }

    static func dragType(_ b: CGMouseButton) -> CGEventType {
        switch b {
        case .left: .leftMouseDragged
        case .right: .rightMouseDragged
        default: .otherMouseDragged
        }
    }

    private static let named: [String: Int] = [
        "return": kVK_Return, "tab": kVK_Tab, "space": kVK_Space, "escape": kVK_Escape,
        "delete": kVK_Delete, "forwardDelete": kVK_ForwardDelete,
        "left": kVK_LeftArrow, "right": kVK_RightArrow, "up": kVK_UpArrow, "down": kVK_DownArrow,
        "home": kVK_Home, "end": kVK_End, "pageUp": kVK_PageUp, "pageDown": kVK_PageDown,
        "f1": kVK_F1, "f2": kVK_F2, "f3": kVK_F3, "f4": kVK_F4, "f5": kVK_F5, "f6": kVK_F6,
        "f7": kVK_F7, "f8": kVK_F8, "f9": kVK_F9, "f10": kVK_F10, "f11": kVK_F11, "f12": kVK_F12,
    ]

    /// Named keys, else the key that types `name` on the current keyboard layout
    /// (so ⌘Z is ⌘Z on AZERTY too).
    static func keyCode(_ name: String) -> CGKeyCode? {
        if let code = named[name] { return CGKeyCode(code) }
        guard name.count == 1 else { return nil }
        return layoutCodes()[name.lowercased()]
    }

    private static func layoutCodes() -> [String: CGKeyCode] {
        var map: [String: CGKeyCode] = [:]
        guard let source = TISCopyCurrentKeyboardLayoutInputSource()?.takeRetainedValue(),
              let raw = TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData)
        else { return map }
        let data = Unmanaged<CFData>.fromOpaque(raw).takeUnretainedValue() as Data
        data.withUnsafeBytes { bytes in
            guard let layout = bytes.baseAddress?.assumingMemoryBound(to: UCKeyboardLayout.self) else { return }
            for code in 0..<128 {
                var dead: UInt32 = 0
                var chars = [UniChar](repeating: 0, count: 4)
                var length = 0
                let status = UCKeyTranslate(layout, UInt16(code), UInt16(kUCKeyActionDown), 0, UInt32(LMGetKbdType()),
                                            OptionBits(kUCKeyTranslateNoDeadKeysMask), &dead, chars.count, &length, &chars)
                guard status == noErr, length > 0 else { continue }
                let s = String(utf16CodeUnits: chars, count: length).lowercased()
                if map[s] == nil { map[s] = CGKeyCode(code) }
            }
        }
        return map
    }
}

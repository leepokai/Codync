#if os(iOS)
import CodyncKit
import SwiftUI
import UIKit
@preconcurrency import WebRTC

public enum TouchMode: String, Sendable {
    /// The phone is a trackpad: drag moves the pointer, tap clicks where it is.
    case trackpad
    /// Tap clicks where you touch.
    case direct
}

/// The video plus every touch and key gesture, in UIKit where gestures and
/// keyboard input are precise. Coordinates leave here as display points.
final class ScreenCanvas: UIView, UIGestureRecognizerDelegate {
    var send: ([String: Any]) -> Void = { _ in }
    /// Asks the computer to stream a region (points; `nil` = everything) at a pixel size.
    var requestView: (CGRect?, CGSize) -> Void = { _, _ in }
    var onArmedConsumed: () -> Void = {}
    var onZoomChanged: (Bool) -> Void = { _ in }

    var mode: TouchMode = .trackpad
    var interactive = true
    /// One-shot modifiers from the on-screen bar, applied to the next key.
    var armed: [String] = []

    private let video = RTCMTLVideoView()
    private let keys = KeyProxyField()
    private weak var track: RTCVideoTrack?
    private var viewport = ScreenViewport(display: CGSize(width: 1, height: 1), view: CGSize(width: 1, height: 1))
    private var streamRegion: CGRect?
    private var cursor = CGPoint.zero
    private var lastTap: (at: Date, point: CGPoint, count: Int)?
    private var dragging = false
    private var pinching = false
    private var viewRequest: Task<Void, Never>?

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .black
        clipsToBounds = true
        video.videoContentMode = .scaleAspectFit
        video.isUserInteractionEnabled = false
        addSubview(video)
        keys.onText = { [weak self] in self?.typed($0) }
        keys.onKey = { [weak self] key, mods in self?.pressKey(key, mods) }
        addSubview(keys)
        installGestures()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    // MARK: state from SwiftUI

    func attach(_ newTrack: RTCVideoTrack?) {
        guard newTrack !== track else { return }
        track?.remove(video)
        track = newTrack
        newTrack?.add(video)
    }

    func setDisplay(_ d: ScreenDisplay) {
        let size = CGSize(width: d.width, height: d.height)
        guard size != viewport.display else { return }
        viewport = ScreenViewport(display: size, view: bounds.size)
        cursor = CGPoint(x: d.width / 2, y: d.height / 2)
        streamRegion = nil
        layoutVideo()
        scheduleViewRequest()
    }

    func setStreamRegion(_ crop: CGRect?) {
        streamRegion = crop
        layoutVideo()
    }

    func setKeyboard(_ shown: Bool) {
        keys.showsSoftKeyboard = shown
    }

    func resetZoom() {
        viewport.reset()
        viewportChanged()
    }

    // MARK: layout

    override func layoutSubviews() {
        super.layoutSubviews()
        if viewport.view != bounds.size {
            viewport.view = bounds.size
            viewport.pan(by: .zero)
            scheduleViewRequest()
        }
        keys.frame = CGRect(x: 0, y: 0, width: 1, height: 1)
        layoutVideo()
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        // First responder even with the keyboard hidden, so a hardware keyboard just works.
        if window != nil { keys.becomeFirstResponder() }
    }

    private func layoutVideo() {
        guard bounds.width > 0, bounds.height > 0 else { return }
        video.frame = viewport.frame(of: streamRegion ?? viewport.displayBounds)
    }

    private func viewportChanged() {
        layoutVideo()
        onZoomChanged(viewport.crop != nil)
        scheduleViewRequest()
    }

    /// Streams just the visible region at the view's pixel size, once the gesture settles.
    private func scheduleViewRequest() {
        viewRequest?.cancel()
        viewRequest = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(180))
            guard let self, !Task.isCancelled, !self.pinching else { return }
            let scale = self.window?.screen.scale ?? 3
            self.requestView(self.viewport.crop, CGSize(width: self.bounds.width * scale, height: self.bounds.height * scale))
        }
    }

    // MARK: gestures

    private func installGestures() {
        let pan = UIPanGestureRecognizer(target: self, action: #selector(onePan(_:)))
        pan.maximumNumberOfTouches = 1
        let tap = UITapGestureRecognizer(target: self, action: #selector(tap(_:)))
        let rightTap = UITapGestureRecognizer(target: self, action: #selector(twoFingerTap(_:)))
        rightTap.numberOfTouchesRequired = 2
        let press = UILongPressGestureRecognizer(target: self, action: #selector(press(_:)))
        press.minimumPressDuration = 0.35
        let scroll = UIPanGestureRecognizer(target: self, action: #selector(twoPan(_:)))
        scroll.minimumNumberOfTouches = 2
        scroll.maximumNumberOfTouches = 2
        let pinch = UIPinchGestureRecognizer(target: self, action: #selector(pinch(_:)))
        for g in [pan, tap, rightTap, press, scroll, pinch] as [UIGestureRecognizer] {
            g.delegate = self
            addGestureRecognizer(g)
        }
        pan.require(toFail: press)
        tap.require(toFail: rightTap)
    }

    func gestureRecognizer(_ g: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool {
        // Pinch and two-finger pan together: zoom and move the view.
        (g is UIPinchGestureRecognizer && other is UIPanGestureRecognizer) || (g is UIPanGestureRecognizer && other is UIPinchGestureRecognizer)
    }

    @objc private func onePan(_ g: UIPanGestureRecognizer) {
        let delta = g.translation(in: self)
        g.setTranslation(.zero, in: self)
        guard interactive else {
            viewport.pan(by: delta)
            viewportChanged()
            return
        }
        switch mode {
        case .trackpad:
            // Pointer speed grows with finger speed, like a real trackpad.
            let speed = g.velocity(in: self)
            let accel = min(2.5, max(1, hypot(speed.x, speed.y) / 900))
            moveCursor(by: CGPoint(x: delta.x * accel / viewport.scale, y: delta.y * accel / viewport.scale))
        case .direct:
            if viewport.crop != nil {
                viewport.pan(by: delta)
                viewportChanged()
            } else {
                send(["type": "scroll", "x": cursor.x, "y": cursor.y, "dx": -delta.x, "dy": -delta.y, "units": "pixel"])
            }
        }
    }

    private func moveCursor(by d: CGPoint) {
        let bounds = viewport.displayBounds
        cursor = CGPoint(x: min(max(cursor.x + d.x, 0), bounds.width - 1), y: min(max(cursor.y + d.y, 0), bounds.height - 1))
        send(["type": "move", "x": cursor.x, "y": cursor.y])
        let before = viewport
        viewport.follow(cursor)
        if viewport != before { viewportChanged() }
    }

    private func point(for g: UIGestureRecognizer) -> CGPoint {
        mode == .direct ? clampToDisplay(viewport.toDisplay(g.location(in: self))) : cursor
    }

    private func clampToDisplay(_ p: CGPoint) -> CGPoint {
        CGPoint(x: min(max(p.x, 0), viewport.display.width - 1), y: min(max(p.y, 0), viewport.display.height - 1))
    }

    @objc private func tap(_ g: UITapGestureRecognizer) {
        guard interactive else { return }
        let p = point(for: g)
        cursor = p
        // Successive quick taps become double/triple clicks without waiting to tell them apart.
        let now = Date()
        var count = 1
        if let last = lastTap, now.timeIntervalSince(last.at) < 0.35, hypot(last.point.x - p.x, last.point.y - p.y) < 8 / viewport.scale {
            count = min(last.count + 1, 3)
        }
        lastTap = (now, p, count)
        send(["type": "click", "x": p.x, "y": p.y, "button": "left", "count": count, "modifiers": consumeArmed()])
    }

    @objc private func twoFingerTap(_ g: UITapGestureRecognizer) {
        guard interactive else { return }
        let p = point(for: g)
        send(["type": "click", "x": p.x, "y": p.y, "button": "right", "count": 1, "modifiers": consumeArmed()])
    }

    /// Long-press then move: drag (select text, move windows).
    @objc private func press(_ g: UILongPressGestureRecognizer) {
        guard interactive else { return }
        switch g.state {
        case .began:
            let p = point(for: g)
            cursor = p
            dragging = true
            UIImpactFeedbackGenerator(style: .medium).impactOccurred()
            send(["type": "down", "x": p.x, "y": p.y, "button": "left"])
            lastPress = g.location(in: self)
        case .changed:
            let loc = g.location(in: self)
            if mode == .direct {
                cursor = clampToDisplay(viewport.toDisplay(loc))
                send(["type": "move", "x": cursor.x, "y": cursor.y])
            } else {
                moveCursor(by: CGPoint(x: (loc.x - lastPress.x) / viewport.scale, y: (loc.y - lastPress.y) / viewport.scale))
            }
            lastPress = loc
        default:
            if dragging {
                send(["type": "up", "x": cursor.x, "y": cursor.y, "button": "left"])
                dragging = false
            }
        }
    }

    private var lastPress = CGPoint.zero

    @objc private func twoPan(_ g: UIPanGestureRecognizer) {
        let delta = g.translation(in: self)
        g.setTranslation(.zero, in: self)
        if pinching || !interactive {
            viewport.pan(by: delta)
            viewportChanged()
            return
        }
        // Natural scrolling: content follows the fingers.
        let at = mode == .direct ? clampToDisplay(viewport.toDisplay(g.location(in: self))) : cursor
        send(["type": "scroll", "x": at.x, "y": at.y, "dx": -delta.x * 1.5, "dy": -delta.y * 1.5, "units": "pixel"])
    }

    @objc private func pinch(_ g: UIPinchGestureRecognizer) {
        switch g.state {
        case .began:
            pinching = true
        case .changed:
            viewport.zoom(by: g.scale, at: g.location(in: self))
            g.scale = 1
            layoutVideo()
        default:
            pinching = false
            viewportChanged()
        }
    }

    // MARK: keys

    private func consumeArmed() -> [String] {
        let mods = armed
        if !mods.isEmpty {
            armed = []
            onArmedConsumed()
        }
        return mods
    }

    private func typed(_ text: String) {
        guard interactive else { return }
        let mods = consumeArmed()
        if !mods.isEmpty, text.count == 1 {
            send(["type": "key", "key": text.lowercased(), "modifiers": mods])
        } else {
            send(["type": "text", "text": text])
        }
    }

    func pressKey(_ key: String, _ modifiers: [String] = []) {
        guard interactive else { return }
        let mods = Array(Set(modifiers + consumeArmed()))
        send(["type": "key", "key": key, "modifiers": mods])
    }
}

/// Invisible text field that turns the iOS keyboard (including IME input such as
/// Zhuyin or Pinyin) and hardware keys into events for the computer.
private final class KeyProxyField: UITextField, UITextFieldDelegate {
    var onText: (String) -> Void = { _ in }
    var onKey: (String, [String]) -> Void = { _, _ in }

    /// With the soft keyboard hidden the field stays first responder, so hardware keys still arrive.
    var showsSoftKeyboard = false {
        didSet {
            guard showsSoftKeyboard != oldValue else { return }
            inputView = showsSoftKeyboard ? nil : UIView()
            reloadInputViews()
            if !isFirstResponder { becomeFirstResponder() }
        }
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        delegate = self
        alpha = 0.02
        tintColor = .clear
        autocorrectionType = .no
        autocapitalizationType = .none
        spellCheckingType = .no
        smartQuotesType = .no
        smartDashesType = .no
        smartInsertDeleteType = .no
        inlinePredictionType = .no
        inputView = UIView()
        inputAssistantItem.leadingBarButtonGroups = []
        inputAssistantItem.trailingBarButtonGroups = []
        addTarget(self, action: #selector(changed), for: .editingChanged)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    /// Sends committed text; text still being composed by an IME waits.
    @objc private func changed() {
        guard markedTextRange == nil, let t = text, !t.isEmpty else { return }
        onText(t)
        text = ""
    }

    override func deleteBackward() {
        if (text ?? "").isEmpty { onKey("delete", []) }
        super.deleteBackward()
    }

    func textFieldShouldReturn(_ textField: UITextField) -> Bool {
        onKey("return", [])
        return false
    }

    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        var unhandled = Set<UIPress>()
        for press in presses {
            guard let key = press.key, let (name, mods) = Self.map(key) else {
                unhandled.insert(press)
                continue
            }
            onKey(name, mods)
        }
        if !unhandled.isEmpty { super.pressesBegan(unhandled, with: event) }
    }

    /// Shortcuts and special keys go straight through; plain typing takes the text path (IME-aware).
    private static func map(_ key: UIKey) -> (String, [String])? {
        var mods: [String] = []
        if key.modifierFlags.contains(.command) { mods.append("cmd") }
        if key.modifierFlags.contains(.alternate) { mods.append("option") }
        if key.modifierFlags.contains(.control) { mods.append("ctrl") }
        if key.modifierFlags.contains(.shift) { mods.append("shift") }
        if let special = special[key.keyCode] { return (special, mods) }
        let chord = mods.contains("cmd") || mods.contains("ctrl") || mods.contains("option")
        guard chord, let c = key.charactersIgnoringModifiers.lowercased().first else { return nil }
        return (String(c), mods)
    }

    private static let special: [UIKeyboardHIDUsage: String] = [
        .keyboardUpArrow: "up", .keyboardDownArrow: "down", .keyboardLeftArrow: "left", .keyboardRightArrow: "right",
        .keyboardEscape: "escape", .keyboardTab: "tab", .keyboardReturnOrEnter: "return", .keypadEnter: "return",
        .keyboardDeleteOrBackspace: "delete", .keyboardDeleteForward: "forwardDelete",
        .keyboardHome: "home", .keyboardEnd: "end", .keyboardPageUp: "pageUp", .keyboardPageDown: "pageDown",
        .keyboardF1: "f1", .keyboardF2: "f2", .keyboardF3: "f3", .keyboardF4: "f4", .keyboardF5: "f5", .keyboardF6: "f6",
        .keyboardF7: "f7", .keyboardF8: "f8", .keyboardF9: "f9", .keyboardF10: "f10", .keyboardF11: "f11", .keyboardF12: "f12",
    ]
}

/// SwiftUI host for the canvas.
struct ScreenCanvasView: UIViewRepresentable {
    let session: ScreenSession
    let display: ScreenDisplay
    let mode: TouchMode
    let interactive: Bool
    let keyboard: Bool
    @Binding var armed: [String]
    @Binding var zoomed: Bool
    let resetToken: Int

    func makeUIView(context: Context) -> ScreenCanvas {
        let canvas = ScreenCanvas()
        canvas.send = { [weak session] in session?.send($0) }
        canvas.requestView = { [weak session] crop, pixels in session?.view(crop: crop, pixels: pixels) }
        return canvas
    }

    func updateUIView(_ canvas: ScreenCanvas, context: Context) {
        canvas.onArmedConsumed = { armed = [] }
        canvas.onZoomChanged = { z in if zoomed != z { zoomed = z } }
        canvas.attach(session.track)
        canvas.setDisplay(display)
        canvas.setStreamRegion(session.streamCrop)
        canvas.mode = mode
        canvas.interactive = interactive
        canvas.armed = armed
        canvas.setKeyboard(keyboard)
        if context.coordinator.resetToken != resetToken {
            context.coordinator.resetToken = resetToken
            canvas.resetZoom()
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator(resetToken: resetToken) }

    final class Coordinator {
        var resetToken: Int
        init(resetToken: Int) { self.resetToken = resetToken }
    }

    static func dismantleUIView(_ canvas: ScreenCanvas, coordinator: Coordinator) {
        canvas.attach(nil)
    }
}
#endif

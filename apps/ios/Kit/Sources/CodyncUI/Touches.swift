import SwiftUI
import UIKit
import UIKit.UIGestureRecognizerSubclass

/// When the app was last touched, anywhere in it. `.recordsTouches()` on the app's root puts one recognizer on its
/// window: it sees every touch there (sheets and full-screen layers included) and never takes one from the views under it.
@MainActor enum Touches {
    private(set) static var last = Date.now

    static func touched() { last = .now }
}

extension View {
    /// Records every touch in this view's window into `Touches`.
    public func recordsTouches() -> some View {
        background(TouchRecorder().frame(width: 0, height: 0))
    }
}

private struct TouchRecorder: UIViewRepresentable {
    func makeUIView(context: Context) -> Probe { Probe() }
    func updateUIView(_ view: Probe, context: Context) {}

    final class Probe: UIView {
        override func didMoveToWindow() {
            super.didMoveToWindow()
            guard let window, !(window.gestureRecognizers ?? []).contains(where: { $0 is Recorder }) else { return }
            window.addGestureRecognizer(Recorder())
        }
    }

    /// Never recognizes, never delays or cancels a touch, and no other gesture keeps it from seeing one.
    final class Recorder: UIGestureRecognizer {
        init() {
            super.init(target: nil, action: nil)
            cancelsTouchesInView = false
            delaysTouchesBegan = false
            delaysTouchesEnded = false
        }

        override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) { Touches.touched() }
        override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) { Touches.touched() }

        override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
            Touches.touched()
            state = .failed
        }

        override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) { state = .failed }
        override func canPrevent(_ other: UIGestureRecognizer) -> Bool { false }
        override func canBePrevented(by other: UIGestureRecognizer) -> Bool { false }
    }
}

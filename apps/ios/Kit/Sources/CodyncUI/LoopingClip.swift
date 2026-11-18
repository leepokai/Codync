import AVFoundation
import SwiftUI

/// A short silent clip that plays in a loop, filling its frame (What's new). With Reduce Motion it rests on its
/// first frame.
struct LoopingClip: UIViewRepresentable {
    let url: URL

    func makeUIView(context: Context) -> ClipView { ClipView(url: url) }
    func updateUIView(_ view: ClipView, context: Context) {}

    final class ClipView: UIView {
        override static var layerClass: AnyClass { AVPlayerLayer.self }
        private let player = AVQueuePlayer()
        private let looper: AVPlayerLooper

        init(url: URL) {
            looper = AVPlayerLooper(player: player, templateItem: AVPlayerItem(url: url))
            super.init(frame: .zero)
            player.isMuted = true
            player.preventsDisplaySleepDuringVideoPlayback = false
            guard let layer = layer as? AVPlayerLayer else { return }
            layer.player = player
            layer.videoGravity = .resizeAspectFill
            // The system pauses a player that shows video when the app leaves the foreground, and never resumes it.
            NotificationCenter.default.addObserver(self, selector: #selector(resume), name: UIApplication.willEnterForegroundNotification, object: nil)
        }

        @available(*, unavailable)
        required init?(coder: NSCoder) { fatalError("made in code") }

        override func didMoveToWindow() { resume() }

        /// Plays only while it's in a window.
        @objc private func resume() {
            if window != nil, !UIAccessibility.isReduceMotionEnabled { player.play() } else { player.pause() }
        }
    }
}

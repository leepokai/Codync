import SwiftUI
import UIKit
@preconcurrency import WebRTC

/// A screen session's video, drawn with Metal: the full viewer lays it out over the region it shows,
/// the bot details' preview shows it whole (`ScreenVideo`).
final class ScreenVideoView: RTCMTLVideoView {
    private weak var track: RTCVideoTrack?

    /// Shows `newTrack`, leaving the one shown before.
    func attach(_ newTrack: RTCVideoTrack?) {
        guard newTrack !== track else { return }
        track?.remove(self)
        track = newTrack
        newTrack?.add(self)
    }
}

/// A session's video fit inside its frame: view only, no gestures or keyboard.
struct ScreenVideo: UIViewRepresentable {
    let track: RTCVideoTrack?

    func makeUIView(context: Context) -> ScreenVideoView {
        let video = ScreenVideoView()
        video.videoContentMode = .scaleAspectFit
        video.isUserInteractionEnabled = false
        return video
    }

    func updateUIView(_ video: ScreenVideoView, context: Context) {
        video.attach(track)
    }

    static func dismantleUIView(_ video: ScreenVideoView, coordinator: ()) {
        video.attach(nil)
    }
}

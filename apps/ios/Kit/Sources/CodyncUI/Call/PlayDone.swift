import AVFoundation

/// Player delegate: tells its owner a clip ended, `true` played to its end (a call's next clip, a voice note's
/// Play again), `false` broke while decoding.
final class PlayDone: NSObject, AVAudioPlayerDelegate, Sendable {
    let onEnd: @Sendable (_ played: Bool) -> Void

    init(onEnd: @escaping @Sendable (_ played: Bool) -> Void) { self.onEnd = onEnd }

    func audioPlayerDidFinishPlaying(_ player: AVAudioPlayer, successfully flag: Bool) { onEnd(flag) }

    func audioPlayerDecodeErrorDidOccur(_ player: AVAudioPlayer, error: Error?) { onEnd(false) }
}

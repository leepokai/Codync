import AVFoundation

/// Player delegate: tells its owner a clip played to its end (a call's next clip, a voice note's Play again).
final class PlayDone: NSObject, AVAudioPlayerDelegate, Sendable {
    let onFinish: @Sendable () -> Void

    init(onFinish: @escaping @Sendable () -> Void) { self.onFinish = onFinish }

    func audioPlayerDidFinishPlaying(_ player: AVAudioPlayer, successfully flag: Bool) { onFinish() }
}

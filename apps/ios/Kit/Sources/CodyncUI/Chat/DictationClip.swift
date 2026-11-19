#if DEBUG
import Foundation

/// Debug builds only: a recorded clip in place of the microphone, for end-to-end checks on a
/// simulator without a usable microphone. `CODYNC_DICTATION_CLIP` names a 16 kHz mono PCM16 WAV;
/// its samples reach dictation in real time, as the microphone's would. Nothing is played aloud
/// and the audio session stays closed.
enum DictationClip {
    /// 100 ms of audio per delivery, like a microphone tap.
    private static let chunkBytes = MicTap.bytesPerSecond / 10

    /// The clip named in the environment, when there is one.
    static var fromEnvironment: Data? {
        guard let path = ProcessInfo.processInfo.environment["CODYNC_DICTATION_CLIP"],
              let wav = FileManager.default.contents(atPath: path) else { return nil }
        return samples(of: wav)
    }

    /// Hands `heard` the clip in 100 ms pieces; dictation drops what comes after its stop.
    static func play(_ pcm: Data, into heard: @escaping @Sendable (Data, Float) -> Void) {
        Task.detached {
            for start in stride(from: 0, to: pcm.count, by: chunkBytes) {
                heard(pcm.subdata(in: start..<min(start + chunkBytes, pcm.count)), 0.5)
                try? await Task.sleep(for: .milliseconds(100))
            }
        }
    }

    /// A WAV's `data` chunk.
    private static func samples(of wav: Data) -> Data? {
        let bytes = [UInt8](wav)
        var offset = 12
        while offset + 8 <= bytes.count {
            let size = Int(bytes[offset + 4]) | Int(bytes[offset + 5]) << 8 | Int(bytes[offset + 6]) << 16 | Int(bytes[offset + 7]) << 24
            if bytes[offset..<offset + 4].elementsEqual("data".utf8) {
                return Data(bytes[offset + 8..<min(offset + 8 + size, bytes.count)])
            }
            offset += 8 + size + size % 2
        }
        return nil
    }
}
#endif

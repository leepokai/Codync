import CodyncKit
import SwiftUI

/// Dictation in progress, in place of the composer's mic: stop, the time since it started, and
/// dots that follow the microphone's level (the desktop composer's pill); a spinner once a
/// recording waits for its transcript.
struct RecordingPill: View {
    let dictation: Dictation
    let height: CGFloat
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        HStack(spacing: 6) {
            if dictation.transcribing {
                Spinner(size: 14)
                    .frame(width: height - 8, height: height - 8)
                Text("Transcribing…")
                    .font(.subheadline)
                    .foregroundStyle(Palette.secondary)
            } else {
                Button { dictation.finish() } label: {
                    Image(systemName: "stop.fill")
                        .font(.system(size: 11, weight: .bold))
                        .foregroundStyle(Palette.text)
                        .frame(width: height - 8, height: height - 8)
                        .contentShape(Circle())
                }
                .buttonStyle(PressScale())
                .accessibilityLabel("Stop dictating")
                .help("Stop dictating")
                Text(timerInterval: dictation.startedAt...Date.distantFuture, countsDown: false)
                    .font(.subheadline.monospacedDigit())
                    .foregroundStyle(Palette.text)
                LevelDots(level: dictation.level, speaking: false, working: false, color: Palette.text, reduceMotion: reduceMotion)
                    .frame(width: 32, height: 12)
                    .accessibilityLabel("Listening")
            }
        }
        .padding(.leading, 4)
        .padding(.trailing, 10)
        .frame(height: height)
        .background(Palette.bubbleAgent, in: Capsule())
    }
}

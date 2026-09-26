import CodyncKit
import CodyncUI
import SwiftUI

/// First launch, before any setup: who the bots are, a glimpse of talking to one, one way forward.
struct WelcomeView: View {
    let start: () -> Void
    @Environment(AccountSession.self) private var account
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// 0 = nothing yet … 4 = everything shown. Each beat enters in turn.
    @State private var beat = 0

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Spacer(minLength: 24)

            Crew(shown: beat >= 1)
                .frame(maxWidth: .infinity)

            Spacer(minLength: 24)

            ChatGlimpse(shown: beat >= 2)
                .padding(.bottom, 32)

            VStack(alignment: .leading, spacing: 10) {
                Text("Your coding agents,\nas teammates.")
                    .font(.system(size: 34, weight: .semibold))
                    .tracking(-0.6)
                    .foregroundStyle(Palette.text)
                Text("Give each one a name and a project. They work on your computer while you're away.")
                    .font(.body)
                    .foregroundStyle(Palette.secondary)
            }
            .rise(beat >= 3)

            Spacer(minLength: 32)

            VStack(spacing: 10) {
                Button(action: start) {
                    Text("Get started").font(.headline).frame(maxWidth: .infinity, minHeight: 50)
                }
                .buttonStyle(.primary)
                // Signed in, the computers on the account show up without scanning a code.
                if account.isConfigured {
                    Button { Task { await account.signIn() } } label: {
                        ZStack {
                            Label("Continue with Google", systemImage: "person.crop.circle")
                                .opacity(account.isBusy ? 0 : 1)
                            if account.isBusy { Spinner(size: 18) }
                        }
                        .font(.headline)
                        .frame(maxWidth: .infinity, minHeight: 50)
                    }
                    .buttonStyle(.secondary)
                    .disabled(account.isBusy)
                    .animation(Motion.fade, value: account.isBusy)
                }
                if let message = account.errorMessage {
                    Text(message).font(.footnote).foregroundStyle(Palette.danger)
                        .transition(.opacity)
                }
            }
            .rise(beat >= 4)
        }
        .padding(.horizontal, 24)
        .padding(.bottom, 12)
        .background(Palette.background)
        .task {
            if reduceMotion { beat = 4; return }
            for next in 1...4 {
                try? await Task.sleep(for: .milliseconds(next == 1 ? 150 : 280))
                withAnimation(.spring(duration: 0.6, bounce: 0.25)) { beat = next }
            }
        }
    }
}

/// Five bots bobbing out of step: the "persistent, named" part of the pitch, without words.
private struct Crew: View {
    let shown: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private let members: [(shape: String, color: String, size: CGFloat, x: CGFloat, y: CGFloat, mood: CharacterAvatar.Mood)] = [
        ("blob", "blue", 92, 0, 0, .working),
        ("squircle", "orange", 64, -104, -34, .idle),
        ("teardrop", "violet", 58, 100, -46, .working),
        ("hex", "green", 48, -76, 64, .idle),
        ("cloud", "magenta", 52, 84, 58, .needsInput),
    ]

    var body: some View {
        TimelineView(.animation(paused: reduceMotion || !shown)) { context in
            let t = context.date.timeIntervalSinceReferenceDate
            ZStack {
                ForEach(members.indices, id: \.self) { i in
                    let m = members[i]
                    CharacterAvatar(shape: m.shape, color: m.color, size: m.size, mood: m.mood)
                        .offset(x: m.x, y: m.y + (reduceMotion ? 0 : sin(t * 1.3 + Double(i) * 1.7) * 5))
                        .scaleEffect(shown ? 1 : 0.3)
                        .opacity(shown ? 1 : 0)
                        .animation(.spring(duration: 0.7, bounce: 0.4).delay(Double(i) * 0.07), value: shown)
                }
            }
        }
        .frame(height: 200)
    }
}

/// One exchange, played once: you ask, a bot works, a bot answers.
private struct ChatGlimpse: View {
    let shown: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var replied = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Fix the flaky login test")
                .bubble(Palette.bubbleUser)
                .frame(maxWidth: .infinity, alignment: .trailing)
                .rise(shown)

            HStack(alignment: .bottom, spacing: 8) {
                CharacterAvatar(shape: "blob", color: "blue", size: 28, mood: replied ? .idle : .working)
                if replied {
                    Text("Done. It raced the session refresh; tests pass on fix/login.")
                        .bubble(Palette.bubbleAgent)
                        .transition(.opacity.combined(with: .scale(scale: 0.92, anchor: .bottomLeading)))
                } else {
                    Text("Working…")
                        .font(.subheadline)
                        .foregroundStyle(Palette.tertiary)
                        .transition(.opacity)
                }
            }
            .rise(shown)
        }
        .accessibilityElement(children: .combine)
        .onChange(of: shown) { _, shown in
            guard shown else { return }
            Task {
                try? await Task.sleep(for: .seconds(reduceMotion ? 0 : 1.6))
                withAnimation(Motion.reduced(.spring(duration: 0.45, bounce: 0.2), reduceMotion)) { replied = true }
            }
        }
    }
}

private extension View {
    func bubble(_ fill: Color) -> some View {
        font(.subheadline)
            .foregroundStyle(Palette.text)
            .padding(.horizontal, 14)
            .padding(.vertical, 9)
            .background(fill, in: RoundedRectangle(cornerRadius: 18))
    }

    /// Fades up into place once `shown`.
    func rise(_ shown: Bool) -> some View {
        opacity(shown ? 1 : 0).offset(y: shown ? 0 : 14)
    }
}

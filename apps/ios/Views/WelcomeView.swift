import CodyncKit
import CodyncUI
import SwiftUI

/// Before any setup, and whenever signed out: who the bots are, a glimpse of talking to one, one way forward.
/// With accounts that way is signing in; builds without them go straight on to pairing.
struct WelcomeView: View {
    /// Computers paired before sign-in was required; they move into the account at sign-in.
    var signedOutComputers = 0
    let start: () -> Void
    @Environment(AccountSession.self) private var account
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// 0 = nothing yet … 4 = everything shown. Each beat enters in turn.
    @State private var beat = 0

    var body: some View {
        GeometryReader { geometry in
            ScrollView {
                content
                    .frame(minHeight: geometry.size.height)
            }
            .scrollBounceBehavior(.basedOnSize)
        }
        .background(Palette.background)
        .tracked(.onboardingStepViewed(.welcome))
        .task {
            if reduceMotion { beat = 4; return }
            for next in 1...4 {
                try? await Task.sleep(for: .milliseconds(next == 1 ? 150 : 280))
                withAnimation(.spring(duration: 0.6, bounce: 0.25)) { beat = next }
            }
        }
    }

    private var content: some View {
        VStack(alignment: .leading, spacing: 0) {
            Spacer(minLength: 24)

            Crew(shown: beat >= 1)
                .frame(maxWidth: .infinity)

            Spacer(minLength: 24)

            ChatGlimpse(shown: beat >= 2)
                .padding(.bottom, 24)

            VStack(alignment: .leading, spacing: 10) {
                Text(L10n.tr("Your coding agents,\nas teammates."))
                    .font(.system(size: 34, weight: .semibold))
                    .tracking(-0.6)
                    .foregroundStyle(Palette.text)
                    .fixedSize(horizontal: false, vertical: true)
                Text(L10n.tr("Give each one a name and a project. They work on your computer while you're away."))
                    .font(.body)
                    .foregroundStyle(Palette.secondary)
                DemoButton().padding(.top, 4)
            }
            .rise(beat >= 3)

            Spacer(minLength: 32)

            VStack(spacing: 10) {
                if signedOutComputers > 0 {
                    Text(signedOutComputers == 1
                         ? L10n.tr("Sign in to keep using your computer. It moves into your account, so you won't need to scan again.")
                         : L10n.tr("Sign in to keep using your {count} computers. They move into your account, so you won't need to scan again.")
                            .replacingOccurrences(of: "{count}", with: String(signedOutComputers)))
                        .font(.footnote)
                        .foregroundStyle(Palette.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.bottom, 4)
                }
                if account.isConfigured {
                    SignInButtons().disabled(!account.isReady)
                } else {
                    Button(action: start) {
                        Text(L10n.tr("Get started")).font(.headline).frame(maxWidth: .infinity, minHeight: 50)
                    }
                    .buttonStyle(.primary)
                }
                if let message = account.errorMessage {
                    Text(message).font(.footnote).foregroundStyle(Palette.danger)
                        .transition(.opacity)
                }
                if account.restoreFailed {
                    HStack(spacing: 8) {
                        Text(L10n.tr("Couldn't restore sign-in. Check your connection and retry."))
                            .font(.footnote).foregroundStyle(Palette.danger)
                            .fixedSize(horizontal: false, vertical: true)
                        IconButton("Try again", systemImage: "arrow.clockwise") { Task { await account.retryRestore() } }
                    }
                    .transition(.opacity)
                }
            }
            .animation(Motion.fade, value: account.restoreFailed)
            .rise(beat >= 4)
        }
        .padding(.horizontal, 24)
        .padding(.bottom, 12)
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
        .frame(height: 170)
    }
}

/// One exchange, played once: you ask, a bot works, a bot answers.
private struct ChatGlimpse: View {
    let shown: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var replied = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(L10n.tr("Fix the flaky login test"))
                .bubble(Palette.bubbleUser)
                .frame(maxWidth: .infinity, alignment: .trailing)
                .rise(shown)

            HStack(alignment: .bottom, spacing: 8) {
                CharacterAvatar(shape: "blob", color: "blue", size: 28, mood: replied ? .idle : .working)
                if replied {
                    Text(L10n.tr("Done. It raced the session refresh; tests pass on fix/login."))
                        .bubble(Palette.bubbleAgent)
                        .transition(.opacity.combined(with: .scale(scale: 0.92, anchor: .bottomLeading)))
                } else {
                    Text(L10n.tr("Working…"))
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

/// Opens a short recording of the app in use on YouTube, for anyone who can't pair a computer yet.
struct DemoButton: View {
    var body: some View {
        Link(destination: URL(string: "https://youtube.com/shorts/xtZ5WeyOewU")!) {
            Label(L10n.tr("Watch the demo"), systemImage: "play.circle.fill")
                .font(.subheadline.weight(.medium))
                .foregroundStyle(Palette.text)
        }
        .trackedLinks(.demo)
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

#if os(iOS)
import CodyncKit
import SwiftUI

/// A voice call with a bot: talk hands-free, hear its final replies. Everything else about the
/// turn (tools, thinking) stays in the chat.
struct CallView: View {
    let botId: String
    let close: () -> Void
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var session: CallSession?
    @State private var startedAt = Date.now

    private var bot: Bot? { model.bots[botId] }
    /// The newest final reply; each new one is read aloud.
    private var lastReply: Entry? { model.thread(botId).last { $0.kind == "agent" && $0.data.final == true } }

    var body: some View {
        VStack(spacing: 0) {
            header.padding(.top, 24)
            Spacer()
            center
            Spacer()
            caption
                .frame(maxWidth: .infinity, minHeight: 96, alignment: .top)
                .padding(.horizontal, 32)
            controls.padding(.bottom, 32).padding(.top, 24)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Palette.background.ignoresSafeArea())
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: session?.phase)
        .onAppear {
            let session = CallSession { [model, botId] in model.send($0, to: botId) }
            self.session = session
            Task { await session.start() }
        }
        .onDisappear { session?.end() }
        .onChange(of: lastReply?.id) { _, _ in
            if let text = lastReply?.data.text { session?.speak(text) }
        }
        .onChange(of: bot?.needsInput) { _, needs in
            if needs == true { session?.speak("\(bot?.name ?? "The bot") needs your approval in the chat.") }
        }
    }

    private var header: some View {
        VStack(spacing: 6) {
            Text(bot?.name ?? "")
                .font(.title2.weight(.semibold))
                .foregroundStyle(Palette.text)
            TimelineView(.periodic(from: startedAt, by: 1)) { context in
                Text(status(now: context.date))
                    .font(.subheadline.monospacedDigit())
                    .foregroundStyle(Palette.secondary)
                    .contentTransition(.numericText())
            }
        }
    }

    private var center: some View {
        Button {
            session?.interrupt()
        } label: {
            ZStack {
                if let session, session.phase == .listening, !session.muted {
                    ThinkingOrb(state: .listening, size: 220, color: Palette.tertiary)
                } else if bot?.isWorking == true, session?.phase != .speaking {
                    ThinkingOrb(state: .working, size: 220, color: Palette.tertiary)
                }
                if let bot { CharacterAvatar(bot: bot, size: 120) }
            }
            .frame(width: 240, height: 240)
            .scaleEffect(session?.phase == .speaking && !reduceMotion ? 1.06 : 1)
            .animation(.easeInOut(duration: 0.9).repeatForever(autoreverses: true), value: session?.phase == .speaking)
            .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .disabled(session?.phase != .speaking)
        .accessibilityLabel(session?.phase == .speaking ? "Interrupt" : bot?.name ?? "")
    }

    @ViewBuilder private var caption: some View {
        switch session?.phase {
        case .failed(let message):
            Text(message)
                .font(.callout)
                .foregroundStyle(Palette.danger)
                .multilineTextAlignment(.center)
        case .speaking:
            Text(session?.said ?? "")
                .font(.body)
                .foregroundStyle(Palette.text)
                .multilineTextAlignment(.center)
                .lineLimit(4)
                .truncationMode(.head)
        default:
            Text(session?.heard ?? "")
                .font(.title3)
                .foregroundStyle(Palette.text)
                .multilineTextAlignment(.center)
                .lineLimit(3)
                .truncationMode(.head)
        }
    }

    private var controls: some View {
        HStack(spacing: 56) {
            let muted = session?.muted == true
            Button {
                session?.muted.toggle()
            } label: {
                Image(systemName: muted ? "mic.slash.fill" : "mic.fill")
                    .font(.system(size: 24, weight: .semibold))
                    .frame(width: 72, height: 72)
                    .background(muted ? Palette.accentFill : Palette.surface, in: Circle())
                    .foregroundStyle(muted ? Palette.onAccent : Palette.text)
                    .contentTransition(.symbolEffect(.replace))
            }
            .buttonStyle(PressScale())
            .accessibilityLabel(muted ? "Unmute" : "Mute")
            Button(action: close) {
                Image(systemName: "phone.down.fill")
                    .font(.system(size: 26, weight: .semibold))
                    .frame(width: 72, height: 72)
                    .background(Palette.danger, in: Circle())
                    .foregroundStyle(.white)
            }
            .buttonStyle(PressScale())
            .accessibilityLabel("End call")
        }
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: session?.muted)
    }

    private func status(now: Date) -> String {
        let seconds = Int(now.timeIntervalSince(startedAt))
        let clock = String(format: "%d:%02d", seconds / 60, seconds % 60)
        let state: String = switch session?.phase {
        case .starting, nil: "Connecting…"
        case .speaking: "Speaking"
        case .failed: "Can't listen"
        case .listening where session?.muted == true: "Muted"
        case .listening: bot?.isWorking == true ? "Working · listening" : "Listening"
        }
        return "\(state) · \(clock)"
    }
}
#endif

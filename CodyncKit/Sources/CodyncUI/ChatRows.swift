import CodyncKit
import SwiftUI

/// The rows a conversation is made of.

struct UserBubble: View {
    let entry: Entry
    let botWorking: Bool
    @Environment(BotStore.self) private var model

    var body: some View {
        VStack(alignment: .trailing, spacing: 4) {
            Text(entry.data.text ?? "")
                .font(.body)
                .foregroundStyle(Palette.onAccent)
                .textSelection(.enabled)
                .padding(.horizontal, 13)
                .padding(.vertical, 9)
                .background(Palette.accentFill, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                .contextMenu {
                    Button("Copy", systemImage: "doc.on.doc") { Pasteboard.copy(entry.data.text) }
                }
            status
        }
        .frame(maxWidth: .infinity, alignment: .trailing)
        .padding(.leading, 48)
    }

    @ViewBuilder private var status: some View {
        switch entry.data.status {
        case "sending":
            Text("Sending…").font(.caption2).foregroundStyle(Palette.tertiary)
        case "queued" where botWorking:
            Text("Waiting to send — it'll read this when it's done").font(.caption2).foregroundStyle(Palette.tertiary)
        case "failed":
            HStack(spacing: 10) {
                Text("Failed to send").foregroundStyle(Palette.danger)
                Button("Resend", systemImage: "arrow.clockwise") { model.retry(entry) }.labelStyle(.iconOnly).help("Resend")
                Button("Delete", systemImage: "trash") { model.discard(entry) }.labelStyle(.iconOnly).help("Delete")
            }
            .font(.caption2.bold())
        case "cancelled":
            Text("Not sent — stopped").font(.caption2).foregroundStyle(Palette.tertiary)
        default:
            EmptyView()
        }
    }
}

struct AgentBubble: View {
    let entry: Entry
    let bot: Bot?
    let showAvatar: Bool
    let openTrace: () -> Void

    var body: some View {
        HStack(alignment: .bottom, spacing: 8) {
            Group {
                if showAvatar, let bot {
                    CharacterAvatar(bot: bot, size: 28, animated: false)
                } else {
                    Color.clear
                }
            }
            .frame(width: 28, height: 28)
            MarkdownText(entry.data.text ?? "")
                .padding(.horizontal, 13)
                .padding(.vertical, 9)
                .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                .contextMenu {
                    Button("Copy", systemImage: "doc.on.doc") { Pasteboard.copy(entry.data.text) }
                    Button("Show what it did", systemImage: "list.bullet.rectangle", action: openTrace)
                }
            Spacer(minLength: 24)
        }
    }
}

struct NoticeRow: View {
    let entry: Entry

    var body: some View {
        let text = entry.data.text ?? ""
        switch entry.data.style {
        case "divider":
            HStack(spacing: 10) {
                Rectangle().fill(Palette.border).frame(height: 1)
                Text(text)
                    .font(.caption2)
                    .foregroundStyle(Palette.tertiary)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: 260)
                    .layoutPriority(1)
                Rectangle().fill(Palette.border).frame(height: 1)
            }
            .padding(.vertical, 6)
        case "error":
            Label {
                Text(text).font(.footnote).foregroundStyle(Palette.text).textSelection(.enabled)
            } icon: {
                Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(Palette.danger)
            }
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.danger.opacity(0.12), in: RoundedRectangle(cornerRadius: 12))
        default:
            Text(text)
                .font(.caption)
                .foregroundStyle(Palette.tertiary)
                .frame(maxWidth: .infinity)
        }
    }
}

/// Grok Bot's typing bubble, plus the live activity line and elapsed time.
struct WorkingIndicator: View {
    let bot: Bot
    let openTrace: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: 8) {
            CharacterAvatar(bot: bot, size: 28)
            VStack(alignment: .leading, spacing: 4) {
                TypingDots()
                    .padding(.horizontal, 12)
                    .padding(.vertical, 11)
                    .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                Button(action: openTrace) {
                    HStack(spacing: 4) {
                        Text(bot.activity.isEmpty ? "Working…" : bot.activity).lineLimit(1)
                        if let started = bot.startedAt {
                            Text("·")
                            Text(Date(milliseconds: started), style: .timer).monospacedDigit()
                        }
                        Image(systemName: "chevron.right").font(.caption2)
                    }
                    .font(.caption)
                    .foregroundStyle(bot.needsInput ? Palette.warning : Palette.secondary)
                }
                .buttonStyle(.plain)
            }
            Spacer()
        }
    }
}

struct TypingDots: View {
    var body: some View {
        HStack(spacing: 4) {
            ForEach(0..<3, id: \.self) { i in
                Circle()
                    .fill(Palette.secondary)
                    .frame(width: 6, height: 6)
                    .phaseAnimator([0.3, 1.0]) { view, phase in
                        view.opacity(phase)
                    } animation: { _ in .easeInOut(duration: 0.5).delay(Double(i) * 0.15) }
            }
        }
    }
}

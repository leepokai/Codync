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
                .font(InterfaceMetrics.body)
                .foregroundStyle(Palette.text)
                .textSelection(.enabled)
                .padding(.horizontal, InterfaceMetrics.value(mac: 12, mobile: 16))
                .padding(.vertical, InterfaceMetrics.value(mac: 8, mobile: 10))
                .background(Palette.bubbleUser, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
                .contextMenu {
                    Button("Copy", systemImage: "doc.on.doc") { Pasteboard.copy(entry.data.text) }
                }
            status
        }
        .frame(maxWidth: .infinity, alignment: .trailing)
        .padding(.leading, 56)
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
            #if os(macOS)
                Text(entry.date, style: .time).font(.system(size: 10)).foregroundStyle(Palette.tertiary)
            #else
                EmptyView()
            #endif
        }
    }
}

struct AgentBubble: View {
    let entry: Entry
    let openTrace: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            MarkdownText(entry.data.text ?? "")
                .padding(.horizontal, InterfaceMetrics.value(mac: 12, mobile: 16))
                .padding(.vertical, InterfaceMetrics.value(mac: 8, mobile: 10))
                .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
                .contextMenu {
                    Button("Copy", systemImage: "doc.on.doc") { Pasteboard.copy(entry.data.text) }
                    Button("Show what it did", systemImage: "list.bullet.rectangle", action: openTrace)
                }
            #if os(macOS)
                Text(entry.date, style: .time)
                    .font(.system(size: 10))
                    .foregroundStyle(Palette.tertiary)
                    .padding(.leading, 12)
            #endif
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        #if os(macOS)
            .padding(.trailing, 64)
        #else
            .padding(.trailing, 40)
        #endif
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
                .font(.footnote)
                .foregroundStyle(Palette.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: .infinity)
        }
    }
}

/// The bot's live activity line: a turning orb, what it's doing and for how long.
struct WorkingIndicator: View {
    let bot: Bot
    let openTrace: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: 10) {
            Button(action: openTrace) {
                HStack(spacing: 8) {
                    ThinkingOrb(size: 16, color: bot.needsInput ? Palette.warning : Palette.secondary)
                    Text(bot.activity.isEmpty ? "Working…" : bot.activity)
                        .font(.subheadline)
                        .foregroundStyle(bot.needsInput ? Palette.warning : Palette.secondary)
                        .lineLimit(1)
                    if let started = bot.startedAt {
                        Text(Date(milliseconds: started), style: .timer)
                            .font(.footnote.monospacedDigit())
                            .foregroundStyle(Palette.tertiary)
                    }
                    Image(systemName: "chevron.right").font(.caption2).foregroundStyle(Palette.tertiary)
                }
                .padding(.horizontal, InterfaceMetrics.value(mac: 12, mobile: 16))
                .padding(.vertical, 12)
                .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
            }
            .buttonStyle(.plain)
            Spacer()
        }
    }
}

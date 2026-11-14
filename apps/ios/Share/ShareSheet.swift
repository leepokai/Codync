import CodyncKit
import SwiftUI

/// The share sheet's Codync: what's being shared, a message, and the chat it goes to (docs/features/share-to-bot.md).
struct ShareSheet: View {
    @Bindable var share: ShareModel
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader("Send to a bot") {
                if share.isSending {
                    Spinner(size: 18).frame(width: 36, height: 36)
                } else {
                    IconButton("Send", systemImage: "arrow.up") { share.send() }
                        .disabled(!share.canSend)
                }
            }
            CardForm {
                if !share.files.isEmpty {
                    CardSection {
                        ForEach(share.files) { FileRow(file: $0) }
                    }
                }
                CardSection {
                    TextField(L10n.tr("Add a message"), text: $share.message, axis: .vertical)
                        .lineLimit(1...6)
                        .disabled(share.isSending)
                }
                status
                targets
            }
        }
        .background(Palette.background)
        .environment(\.dismissModal, DismissModalAction { share.cancel() })
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: share.phase)
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: share.selected)
    }

    @ViewBuilder private var status: some View {
        switch share.phase {
        case let .sending(sent, total) where total > 0:
            VStack(alignment: .leading, spacing: 6) {
                Text(L10n.tr("Uploading {sent} of {total}")
                    .replacingOccurrences(of: "{sent}", with: sent.formatted(.byteCount(style: .file)))
                    .replacingOccurrences(of: "{total}", with: total.formatted(.byteCount(style: .file))))
                UploadBar(fraction: Double(sent) / Double(total))
            }
            .font(.footnote)
            .foregroundStyle(Palette.secondary)
            .transition(.opacity)
        case .sending:
            Text(L10n.tr("Sending…")).font(.footnote).foregroundStyle(Palette.secondary).transition(.opacity)
        case let .failed(message):
            Text(L10n.tr(message)).font(.footnote).foregroundStyle(Palette.danger).transition(.opacity)
        case .loading, .ready:
            EmptyView()
        }
    }

    @ViewBuilder private var targets: some View {
        if share.targets.isEmpty {
            Text(L10n.tr(share.hasComputers ? "No bots yet" : "Open Codync and connect a computer first."))
                .font(.footnote)
                .foregroundStyle(Palette.secondary)
        }
        if !share.targets.recent.isEmpty {
            CardSection("Recent") {
                ForEach(share.targets.recent) { row(for: $0, showsComputer: share.targets.manyComputers) }
            }
        }
        ForEach(share.targets.sections) { section in
            CardSection(section.title ?? "Bots") {
                ForEach(section.rows) { row(for: $0, showsComputer: false) }
            }
        }
    }

    private func row(for row: ShareTargets.Row, showsComputer: Bool) -> some View {
        let picked = share.selected == row.recipient
        return Button {
            share.selected = row.recipient
        } label: {
            HStack(spacing: 12) {
                BotAvatar(bot: row.bot, members: row.members, size: 32, animated: false)
                    .frame(width: 32, height: 32)
                VStack(alignment: .leading, spacing: 2) {
                    Text(row.bot.name).foregroundStyle(Palette.text).lineLimit(1)
                    if showsComputer {
                        Text(row.computerName).font(.footnote).foregroundStyle(Palette.secondary).lineLimit(1)
                    }
                }
                Spacer(minLength: 8)
                Image(systemName: "checkmark")
                    .font(.body.weight(.semibold))
                    .foregroundStyle(Palette.text)
                    .opacity(picked ? 1 : 0)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(share.isSending)
        .accessibilityAddTraits(picked ? .isSelected : [])
    }
}

/// A shared file: its picture or icon, name and size; one that stays behind says why.
private struct FileRow: View {
    let file: IncomingFile

    var body: some View {
        HStack(spacing: 12) {
            Group {
                if let thumbnail = file.thumbnail {
                    Image(uiImage: thumbnail).resizable().scaledToFill()
                } else {
                    Image(systemName: AttachmentIcon.symbol(file.name)).foregroundStyle(Palette.secondary)
                }
            }
            .frame(width: 36, height: 36)
            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
            VStack(alignment: .leading, spacing: 2) {
                Text(file.name).foregroundStyle(Palette.text).lineLimit(1).truncationMode(.middle)
                Group {
                    switch file.content {
                    case let .ready(outgoing):
                        Text(outgoing.data.count.formatted(.byteCount(style: .file))).foregroundStyle(Palette.secondary)
                    case .tooLarge:
                        Text(L10n.tr("Larger than 100 MB")).foregroundStyle(Palette.danger)
                    case .unreadable:
                        Text(L10n.tr("Couldn't read this file")).foregroundStyle(Palette.danger)
                    }
                }
                .font(.footnote)
            }
        }
        .accessibilityElement(children: .combine)
    }
}

/// How much of the files went: a thin bar.
private struct UploadBar: View {
    let fraction: Double

    var body: some View {
        Capsule()
            .fill(Palette.accentDim)
            .frame(height: 4)
            .overlay(alignment: .leading) {
                GeometryReader { space in
                    Capsule().fill(Palette.accent).frame(width: space.size.width * min(max(fraction, 0), 1))
                }
            }
            .accessibilityHidden(true)
    }
}

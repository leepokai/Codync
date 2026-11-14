import CodyncKit
import SwiftUI

/// One layer's long-form notes, under its entries: name, description, size and date, marked past a cap. Read-only:
/// the bot writes them.
struct MemoryFilesList: View {
    let memory: MemoryStore
    let files: MemoryFiles
    let shared: Bool
    @State private var open: MemoryFileTarget?
    @Environment(\.noteLines) private var noteLines

    var body: some View {
        let live = (shared ? files.yours : files.bot).files.filter { !$0.deleted }
        Text(L10n.tr("Notes")).font(.footnote.weight(.semibold)).foregroundStyle(Palette.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.top, 6)
            .codyncSheet(item: $open) { target in MemoryFileView(memory: memory, target: target) }
        if live.isEmpty {
            Text(L10n.tr("Longer notes, one topic each. The bot reads one when its topic comes up."))
                .font(.footnote).foregroundStyle(Palette.tertiary)
        } else if live.count > files.folderCap {
            Text(L10n.tr("\(live.count) notes, over the \(files.folderCap)-note cap. Ask the bot to merge some."))
                .font(.footnote).foregroundStyle(Palette.attention)
        }
        ForEach(live) { file in row(file, cap: files.cap) }
    }

    private func row(_ file: MemoryFile, cap: Int) -> some View {
        Button {
            withAnimation(Motion.layout) { open = MemoryFileTarget(name: file.name, shared: shared) }
        } label: {
            VStack(alignment: .leading, spacing: 4) {
                Label(file.name, systemImage: "doc.text")
                if let description = file.description {
                    Text(description).font(.footnote).foregroundStyle(Palette.secondary).lineLimit(noteLines)
                }
                Text("\(file.tokens) tokens\(file.tokens > cap ? ", over the \(cap) cap" : "") · \(Date(milliseconds: file.updatedAt).formatted(.relative(presentation: .named)))")
                    .font(.caption)
                    .foregroundStyle(file.tokens > cap ? Palette.attention : Palette.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(.vertical, 6)
    }
}

/// The note a sheet opens.
struct MemoryFileTarget: Identifiable {
    let name: String
    let shared: Bool
    var id: String { "\(shared)-\(name)" }
}

/// One note, to read.
struct MemoryFileView: View {
    let memory: MemoryStore
    let target: MemoryFileTarget
    @State private var detail: MemoryFileDetail?

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader(target.name)
            CardForm {
                if let error = memory.error {
                    CardSection { Text(L10n.tr(error)).foregroundStyle(Palette.danger).textSelection(.enabled) }
                }
                if let detail {
                    CardSection(footer: detail.path) {
                        Text(detail.content ?? "Deleted.")
                            .font(.footnote.monospaced())
                            .textSelection(.enabled)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                } else {
                    CardSection { Text(L10n.tr("Loading…")).foregroundStyle(Palette.secondary) }
                }
            }
        }
        .background(Palette.background)
        .task { detail = await memory.file(target.name, shared: target.shared) }
    }
}

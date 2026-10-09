import CodyncKit
import SwiftUI

enum MemoryBackupMode: String, Identifiable { case export, `import`; var id: String { rawValue } }

struct MemoryBackupView: View {
    let memory: MemoryStore
    let mode: MemoryBackupMode
    @State private var text = ""
    @Environment(\.dismissModal) private var dismiss

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader(mode == .export ? "Export memories" : "Import memories") { EmptyView() }
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Text(mode == .export ? "Copy this Engram backup to keep or import your memories." : "Paste an Engram JSON backup. Import merges memories and preserves newer changes. A backup is saved first.")
                    if mode == .export {
                        Text(memory.backup.isEmpty ? "Preparing backup…" : memory.backup).font(.caption.monospaced()).textSelection(.enabled)
                    } else {
                        TextEditor(text: $text).frame(minHeight: 280).scrollContentBackground(.hidden)
                            .padding(10).background(Palette.surface, in: RoundedRectangle(cornerRadius: 12))
                        Button("Import") { Task { if await memory.importBackup(text) { dismiss() } } }
                            .buttonStyle(PrimaryButtonStyle()).disabled(memory.busy || text.isEmpty)
                    }
                    if let error = memory.error { Text(error).foregroundStyle(Palette.danger) }
                }.padding(20)
            }
        }
    }
}

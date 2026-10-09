import CodyncKit
import SwiftUI

struct MemoryHistoryView: View {
    let memory: MemoryStore

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader("Memory history") { EmptyView() }
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    if let error = memory.error { Text(error).foregroundStyle(Palette.danger) }
                    if let detail = memory.detail {
                        Text(detail.history.result).textSelection(.enabled)
                        if let cursor = detail.history.history_cursor {
                            Button("Older versions") { Task { await memory.inspect(detail.fact, cursor: cursor) } }.buttonStyle(.plain)
                        }
                        Text("Session timeline").fontWeight(.semibold)
                        Text(detail.timeline.result).textSelection(.enabled)
                    } else { Text("Loading history…").foregroundStyle(Palette.secondary) }
                }.frame(maxWidth: .infinity, alignment: .leading).padding(20)
            }
        }
    }
}

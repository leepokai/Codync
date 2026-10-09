import CodyncKit
import SwiftUI

struct MemoryEditorView: View {
    let memory: MemoryStore
    @State var draft: MemoryDraft
    @Environment(\.dismissModal) private var dismiss

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader(draft.id == nil ? "Add memory" : "Edit memory") { EmptyView() }
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    field("Title", text: $draft.title)
                    Text("Memory").font(.caption).foregroundStyle(Palette.secondary)
                    TextEditor(text: $draft.content).frame(minHeight: 160).scrollContentBackground(.hidden)
                        .padding(10).background(Palette.surface, in: RoundedRectangle(cornerRadius: 12))
                    HStack(spacing: 14) {
                        ForEach(["project", "personal", "global"], id: \.self) { scope in
                            Button(scope == "personal" ? "About you" : scope == "global" ? "General" : "Work") {
                                withAnimation(Motion.layout) { draft.scope = scope }
                            }
                            .fontWeight(draft.scope == scope ? .semibold : .regular)
                        }
                    }.buttonStyle(.plain)
                    field("Category", text: $draft.memoryType)
                    field("Topic (optional)", text: $draft.topicKey)
                    if let error = memory.error { Text(error).foregroundStyle(Palette.danger) }
                    Button(memory.busy ? "Saving…" : "Save") {
                        Task { if await memory.save(draft) { dismiss() } }
                    }
                    .buttonStyle(PrimaryButtonStyle())
                    .disabled(memory.busy || draft.title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || draft.content.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }.padding(20)
            }
        }
    }

    private func field(_ title: String, text: Binding<String>) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).font(.caption).foregroundStyle(Palette.secondary)
            TextField(title, text: text).textFieldStyle(.plain).plainTextInput()
                .padding(12).background(Palette.surface, in: RoundedRectangle(cornerRadius: 12))
        }
    }
}

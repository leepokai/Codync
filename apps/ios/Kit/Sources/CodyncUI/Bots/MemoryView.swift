import CodyncKit
import SwiftUI

struct MemoryCard: View {
    let botId: String
    @Environment(BotStore.self) private var host
    @State private var memory: MemoryStore?
    @State private var editor: MemoryEditorTarget?
    @State private var showDetail = false
    @State private var backupMode: MemoryBackupMode?
    @State private var confirmClear = false

    var body: some View {
        CardSection("Memory") {
            if let memory { contents(memory) }
            else { Text("Loading memory…").foregroundStyle(Palette.secondary) }
        } accessory: {
            IconButton("Add memory", systemImage: "plus") { openEditor() }
                .disabled(memory == nil || memory?.busy == true)
        }
        .task(id: botId) {
            let store = MemoryStore(botId: botId) { try await host.ready() }
            memory = store
            await store.load()
        }
        .codyncSheet(item: $editor) { target in
            if let memory { MemoryEditorView(memory: memory, draft: target.draft) }
        }
        .codyncSheet(isPresented: $showDetail) {
            if let memory { MemoryHistoryView(memory: memory) }
        }
        .codyncSheet(item: $backupMode) { mode in
            if let memory { MemoryBackupView(memory: memory, mode: mode) }
        }
        .codyncDialog("Forget everything this bot remembers?", isPresented: $confirmClear) {
            [DialogAction("Forget everything", destructive: true) {
                Task { await memory?.action("clearMemory") }
            }]
        }
    }

    @ViewBuilder private func contents(_ memory: MemoryStore) -> some View {
        SearchField("Search memories", text: Binding(get: { memory.query }, set: { memory.search($0) }))
        ScrollView(.horizontal) {
            HStack(spacing: 14) {
                ForEach(["all", "profile", "pinned", "review"], id: \.self) { filter in
                    Button(["all": "All", "profile": "About you", "pinned": "Pinned", "review": "Needs review"][filter] ?? filter) {
                        withAnimation(Motion.layout) { memory.search(memory.query, filter: filter) }
                    }
                    .fontWeight(memory.filter == filter ? .semibold : .regular)
                    .foregroundStyle(memory.filter == filter ? Palette.text : Palette.secondary)
                }
            }
            .buttonStyle(.plain)
        }
        if let error = memory.error { Text(error).foregroundStyle(Palette.danger).textSelection(.enabled) }
        if !memory.loaded && memory.error == nil { Text("Loading memory…").foregroundStyle(Palette.secondary) }
        if memory.loaded && memory.facts.isEmpty {
            Text(memory.query.isEmpty && memory.filter == "all"
                 ? "Nothing yet. Important details are remembered as you chat." : "No matching memories.")
                .foregroundStyle(Palette.secondary)
        }
        ForEach(memory.facts) { fact in
            VStack(alignment: .leading, spacing: 8) {
                Button { openEditor(fact) } label: {
                    VStack(alignment: .leading, spacing: 4) {
                        Text((fact.pinned ? "● " : "") + fact.title).fontWeight(.semibold)
                        Text(fact.content).lineLimit(3)
                        Text("\(fact.kind == "profile" ? "About you" : fact.memoryType) · \(Date(timeIntervalSince1970: Double(fact.createdAt) / 1000).formatted(date: .abbreviated, time: .omitted)) · \(fact.revisionCount) revisions")
                            .font(.caption).foregroundStyle(Palette.secondary)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                ScrollView(.horizontal) {
                    HStack(spacing: 14) {
                        Button(fact.pinned ? "Unpin" : "Pin") { Task { await memory.action("pinMemory", fact: fact) } }
                        Button("History") {
                            withAnimation(Motion.layout) { showDetail = true }
                            Task { await memory.inspect(fact) }
                        }
                        if fact.reviewAfter != nil {
                            Button("Mark reviewed") { Task { await memory.action("reviewMemory", fact: fact) } }
                        }
                        Button("Forget") { Task { await memory.action("forgetMemory", fact: fact) } }
                    }
                    .buttonStyle(.plain).font(.caption).foregroundStyle(Palette.secondary)
                    .disabled(memory.busy)
                }
            }
            .padding(.vertical, 6)
        }
        if memory.nextOffset != nil {
            Button("Load more (\(memory.total))") { Task { await memory.load(more: true) } }.buttonStyle(.plain)
        }
        ScrollView(.horizontal) {
            HStack(spacing: 14) {
                Button("Refresh") { Task { await memory.load() } }
                Button("Export") {
                    withAnimation(Motion.layout) { backupMode = .export }
                    Task { await memory.export() }
                }
                Button("Import") { withAnimation(Motion.layout) { backupMode = .import } }
                Button("Forget everything") { withAnimation(Motion.layout) { confirmClear = true } }
                    .disabled(memory.busy)
            }
            .buttonStyle(.plain).font(.caption).foregroundStyle(Palette.secondary)
        }
    }

    private func openEditor(_ fact: MemoryFact? = nil) {
        withAnimation(Motion.layout) { editor = MemoryEditorTarget(draft: MemoryDraft(botId: botId, fact: fact)) }
    }
}

private struct MemoryEditorTarget: Identifiable {
    let id = UUID()
    var draft: MemoryDraft
}

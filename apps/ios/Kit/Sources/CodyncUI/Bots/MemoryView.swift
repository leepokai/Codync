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
            HStack(spacing: 2) {
                IconButton("Add memory", systemImage: "plus") { openEditor() }
                    .disabled(memory == nil || memory?.busy == true)
                if let memory { moreMenu(memory) }
            }
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
        HStack(spacing: 8) {
            SearchField("Search memories", text: Binding(get: { memory.query }, set: { memory.search($0) }))
            ChoicePicker(selection: Binding(get: { memory.filter }, set: { filter in
                withAnimation(Motion.layout) { memory.search(memory.query, filter: filter) }
            }), options: Self.filters, fill: Palette.bubbleAgent)
        }
        if let error = memory.error { Text(error).foregroundStyle(Palette.danger).textSelection(.enabled) }
        if !memory.loaded && memory.error == nil { Text("Loading memory…").foregroundStyle(Palette.secondary) }
        if memory.loaded && memory.facts.isEmpty {
            VStack(spacing: 8) {
                Image(systemName: "brain").font(.title3).foregroundStyle(Palette.tertiary)
                Text(memory.query.isEmpty && memory.filter == "all"
                     ? "Nothing yet. Important details are remembered as you chat." : "No matching memories.")
                    .multilineTextAlignment(.center)
                    .foregroundStyle(Palette.secondary)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 18)
        }
        ForEach(memory.facts) { fact in
            HStack(alignment: .top, spacing: 8) {
                Button { openEditor(fact) } label: {
                    VStack(alignment: .leading, spacing: 4) {
                        HStack(spacing: 6) {
                            if fact.pinned { Image(systemName: "pin.fill").font(.caption2).foregroundStyle(Palette.secondary) }
                            Text(fact.title).fontWeight(.semibold)
                        }
                        Text(fact.content).lineLimit(3)
                        Text("\(fact.kind == "profile" ? "About you" : fact.memoryType) · \(Date(timeIntervalSince1970: Double(fact.createdAt) / 1000).formatted(date: .abbreviated, time: .omitted)) · \(fact.revisionCount) revisions")
                            .font(.caption).foregroundStyle(Palette.secondary)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                factMenu(memory, fact)
            }
            .padding(.vertical, 6)
        }
        if memory.nextOffset != nil {
            Button("Load more (\(memory.total))") { Task { await memory.load(more: true) } }
                .buttonStyle(.plain).foregroundStyle(Palette.secondary).frame(maxWidth: .infinity)
        }
    }

    private static let filters = [("all", "All"), ("profile", "About you"), ("pinned", "Pinned"), ("review", "Needs review")]
        .map { (id: $0.0, label: $0.1) }

    private func moreMenu(_ memory: MemoryStore) -> some View {
        DropdownMenu {
            [MenuItem("Refresh", icon: "arrow.clockwise") { Task { await memory.load() } },
             MenuItem("Export", icon: "square.and.arrow.up") {
                 withAnimation(Motion.layout) { backupMode = .export }
                 Task { await memory.export() }
             },
             MenuItem("Import", icon: "square.and.arrow.down") { withAnimation(Motion.layout) { backupMode = .import } },
             MenuItem("Forget everything", icon: "trash", destructive: true, divider: true) {
                 withAnimation(Motion.layout) { confirmClear = true }
             }]
        } label: { menuLabel("More") }
    }

    private func factMenu(_ memory: MemoryStore, _ fact: MemoryFact) -> some View {
        DropdownMenu {
            [MenuItem(fact.pinned ? "Unpin" : "Pin", icon: fact.pinned ? "pin.slash" : "pin") {
                Task { await memory.action("pinMemory", fact: fact) }
            },
             MenuItem("History", icon: "clock.arrow.circlepath") {
                 withAnimation(Motion.layout) { showDetail = true }
                 Task { await memory.inspect(fact) }
             }]
            + (fact.reviewAfter != nil ? [MenuItem("Mark reviewed", icon: "checkmark.circle") {
                Task { await memory.action("reviewMemory", fact: fact) }
            }] : [])
            + [MenuItem("Forget", icon: "trash", destructive: true, divider: true) {
                Task { await memory.action("forgetMemory", fact: fact) }
            }]
        } label: { menuLabel("Memory actions") }
        .disabled(memory.busy)
    }

    private func menuLabel(_ title: String) -> some View {
        Image(systemName: "ellipsis")
            .font(.body.weight(.medium))
            .foregroundStyle(Palette.secondary)
            .frame(width: 32, height: 32)
            .contentShape(Rectangle())
            .accessibilityLabel(title)
    }

    private func openEditor(_ fact: MemoryFact? = nil) {
        withAnimation(Motion.layout) { editor = MemoryEditorTarget(draft: MemoryDraft(botId: botId, fact: fact)) }
    }
}

private struct MemoryEditorTarget: Identifiable {
    let id = UUID()
    var draft: MemoryDraft
}

import CodyncKit
import SwiftUI

/// A bot's memory, read-only: what all your bots know about you, what this one knows, its dreams, and its harness's
/// own notes when the user kept them on. The user changes memory by telling the bot.
struct MemoryCard: View {
    let botId: String
    @Environment(BotStore.self) private var host
    @Environment(\.verticalSizeClass) private var verticalSizeClass
    @State private var memory: MemoryStore?
    @State private var section: Section = .bot
    @State private var info: Info?
    /// Entries and dreams showing all their lines.
    @State private var unfolded: Set<Fold> = []

    enum Section: Hashable { case yours, bot, dreams, harness }
    /// What an info button explains.
    enum Info: Identifiable { case harness, dreams; var id: Self { self } }
    /// What a tap unfolds.
    enum Fold: Hashable { case entry(Int64), dream(Int64) }

    private var bot: Bot? { host.bots[botId] }
    /// A long memory stops at a few lines, fewer when the screen is short (landscape); tapping it shows the rest.
    private func lines(_ key: Fold) -> Int? {
        if unfolded.contains(key) { return nil }
        return verticalSizeClass == .compact ? 2 : 5
    }

    private func unfold(_ key: Fold) {
        withAnimation(Motion.layout) { unfolded.formSymmetricDifference([key]) }
    }

    var body: some View {
        VStack(spacing: 10) {
            TabChoice(selection: Binding(get: { section }, set: { value in
                withAnimation(Motion.layout) { section = value }
                // Claude Code writes its notes outside Codync, so nothing moves `memoryRev`: read them again on the way in.
                if value == .harness { Task { await memory?.load() } }
            }), options: sections(memory?.view))
            CardSection {
                if let memory { contents(memory) }
                else { Text(L10n.tr("Loading memory…")).foregroundStyle(Palette.secondary) }
            }
        }
        .tracked(.screenViewed(.memory))
        // Memory changes anywhere move the bot's `memoryRev`: fetch again.
        .task(id: "\(botId)-\(bot?.memoryRev ?? 0)-\(bot?.sharedMemory ?? false)-\(bot?.harnessMemory ?? false)") {
            let store = memory ?? MemoryStore(botId: botId) { try await host.ready() }
            memory = store
            await store.load()
        }
        // Claude Code's section goes when its memory is turned off (on another device too).
        .onChange(of: memory?.view.map { $0.projectMemory == nil } ?? false) { _, gone in
            if gone && section == .harness { section = .bot }
        }
        .codyncSheet(item: $info) { info in
            switch info {
            case .harness:
                if let bot {
                    MemoryInfoSheet.harness(host.backendName(bot.backend), access: bot.harnessMemoryShared == true
                        ? .shared : bot.backend == "claude" ? .readable : .hidden)
                }
            case .dreams: MemoryInfoSheet.dreams
            }
        }
    }

    @ViewBuilder private func contents(_ memory: MemoryStore) -> some View {
        if let error = memory.error { Text(L10n.tr(error)).foregroundStyle(Palette.danger).textSelection(.enabled) }
        if let view = memory.view {
            switch section {
            case .yours, .bot: layer(memory, view: view)
            case .dreams: dreams(view.dreams)
            case .harness: harness(view.projectMemory)
            }
        } else if memory.error == nil {
            Text(L10n.tr("Loading memory…")).foregroundStyle(Palette.secondary)
        }
    }

    private func sections(_ view: MemoryView?) -> [(id: Section, label: String)] {
        [(Section.yours, "About you"), (.bot, "This bot"), (.dreams, "Dreams")]
            + (view?.projectMemory != nil ? [(.harness, host.backendName(bot?.backend ?? "claude"))] : [])
    }

    @ViewBuilder private func layer(_ memory: MemoryStore, view: MemoryView) -> some View {
        let shared = section == .yours
        let layer = shared ? view.yours : view.bot
        if shared, let bot {
            Toggle(isOn: Binding(get: { bot.sharedMemory ?? view.shared }, set: { host.setSharedMemory(bot, $0) })) {
                Text(L10n.tr("Use what your bots share"))
            }
                .toggleStyle(.codync)
        }
        // The harness's own memory: a switch where Codync can turn it off, "Shared" where every bot on it shares it.
        if !shared, let bot, bot.harnessMemory != nil || bot.harnessMemoryShared == true {
            let name = host.backendName(bot.backend)
            let own = L10n.tr("{name}'s own memory").replacingOccurrences(of: "{name}", with: name)
            HStack(spacing: 4) {
                Text(own).lineLimit(1).layoutPriority(1)
                IconButton(L10n.tr("About {name}'s own memory").replacingOccurrences(of: "{name}", with: name), systemImage: "info.circle") {
                    withAnimation(Motion.layout) { info = .harness }
                }
                if let on = bot.harnessMemory {
                    Toggle(isOn: Binding(get: { on }, set: { host.setHarnessMemory(bot, $0) })) { EmptyView() }
                        .toggleStyle(.codync)
                        .accessibilityLabel(own)
                } else {
                    Spacer(minLength: 8)
                    Text(L10n.tr("Shared")).foregroundStyle(Palette.secondary).lineLimit(1)
                }
            }
        }
        Text(L10n.tr(shared
                     ? "Your name, time zone, people and lasting preferences, shared by your bots. {used} of {cap} in use. To change it, tell the bot."
                     : "What this bot learned about you. {used} of {cap} in use. To change it, tell the bot.")
            .replacingOccurrences(of: "{used}", with: "\(layer.used)")
            .replacingOccurrences(of: "{cap}", with: "\(layer.cap)"))
            .font(.footnote).foregroundStyle(Palette.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
        if layer.entries.isEmpty {
            empty("brain", shared ? "Nothing shared yet." : "Nothing yet. What this bot learns about you shows up here.")
        }
        ForEach(layer.entries) { entry in
            VStack(alignment: .leading, spacing: 4) {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    if entry.pinned { Image(systemName: "pin.fill").font(.caption2).foregroundStyle(Palette.secondary) }
                    Text(entry.text).foregroundStyle(entry.archived ? Palette.tertiary : Palette.text).lineLimit(lines(.entry(entry.id)))
                }
                Text("\(author(entry)) · \(Date(milliseconds: entry.updatedAt).formatted(.relative(presentation: .named)))\(entry.archived ? " · archived" : "")")
                    .font(.caption).foregroundStyle(Palette.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
            .onTapGesture { unfold(.entry(entry.id)) }
            .padding(.vertical, 6)
        }
        if let files = view.files { MemoryFilesList(memory: memory, files: files, shared: shared) }
    }

    /// What dreaming did.
    @ViewBuilder private func dreams(_ runs: [DreamRun]) -> some View {
        HStack(alignment: .top, spacing: 8) {
            Text(L10n.tr("Between chats the bot tidies its memory from your own words."))
                .font(.footnote).foregroundStyle(Palette.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            IconButton("About dreaming", systemImage: "info.circle") {
                withAnimation(Motion.layout) { info = .dreams }
            }
        }
        if runs.isEmpty { empty("moon", "No dreams yet.") }
        ForEach(runs) { run in
            VStack(alignment: .leading, spacing: 4) {
                Text("\(Date(milliseconds: run.at).formatted(.relative(presentation: .named)))\(run.undone ? " · undone" : "")")
                    .fontWeight(.semibold)
                let limit = lines(.dream(run.id))
                ForEach(run.added, id: \.id) { Text(L10n.tr("+ \($0.text)")).foregroundStyle(Palette.added).lineLimit(limit) }
                ForEach(run.archived, id: \.id) { Text(L10n.tr("− \($0.text)")).foregroundStyle(Palette.removed).lineLimit(limit) }
                ForEach(run.files ?? [], id: \.version) { Label("Added to \($0.name)", systemImage: "doc.text").foregroundStyle(Palette.secondary) }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
            .onTapGesture { unfold(.dream(run.id)) }
            .padding(.vertical, 6)
        }
    }

    @ViewBuilder private func harness(_ project: MemoryView.Project?) -> some View {
        if let project {
            Text(L10n.tr("Claude Code's own notes, read-only here."))
                .font(.footnote).foregroundStyle(Palette.secondary)
            Text(project.path).font(.caption).foregroundStyle(Palette.tertiary).textSelection(.enabled)
            if project.text.isEmpty { empty("brain", "Nothing yet. Claude Code's notes show up here once it saves some.") }
            else { Text(project.text).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading) }
        }
    }

    private func empty(_ icon: String, _ text: String) -> some View {
        VStack(spacing: 8) {
            Image(systemName: icon).font(.title3).foregroundStyle(Palette.tertiary)
            Text(L10n.tr(text)).multilineTextAlignment(.center).foregroundStyle(Palette.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 18)
    }

    private func author(_ entry: MemoryEntry) -> String {
        switch entry.author {
        case "user": "You wrote it"
        case "bot": "\(host.bots[entry.authorBot ?? ""]?.name ?? "A bot") remembered it"
        case "dream": "Dreaming"
        default: "Moved from the old memory"
        }
    }
}

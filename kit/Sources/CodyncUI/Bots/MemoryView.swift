import CodyncKit
import SwiftUI

/// One remembered fact (host `memory` API).
public struct MemoryFact: Codable, Identifiable, Hashable, Sendable {
    public var id: String
    public var content: String
    public var createdAt: Int64
    /// `profile` (who the user is) or `log` (dated history).
    public var kind: String
}

struct MemoryListing: Codable, Sendable {
    var location: String
    var facts: [MemoryFact]
}

extension HostClient {
    func memory(botId: String) async throws -> MemoryListing {
        struct Body: Encodable { var botId: String }
        return try await call("memory", Body(botId: botId))
    }

    func forgetMemory(botId: String, id: String) async throws {
        struct Body: Encodable { var botId: String; var id: String }
        let _: Empty = try await call("forgetMemory", Body(botId: botId, id: id))
    }

    func clearMemory(botId: String) async throws {
        struct Body: Encodable { var botId: String }
        let _: Empty = try await call("clearMemory", Body(botId: botId))
    }
}

/// What the bot remembers about the user, in the bot's settings. Facts are
/// learned after each exchange and survive new sessions; remove any here.
struct MemoryCard: View {
    let botId: String
    @Environment(BotStore.self) private var model
    @State private var facts: [MemoryFact] = []
    @State private var loaded = false
    @State private var confirmClear = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text("Memory").font(InterfaceMetrics.secondary).foregroundStyle(Palette.secondary).padding(.leading, 4)
                Spacer()
                if !facts.isEmpty {
                    Button("Forget everything", systemImage: "trash") { confirmClear = true }
                        .labelStyle(.iconOnly)
                        .buttonStyle(.plain)
                        .foregroundStyle(Palette.secondary)
                        .help("Forget everything")
                }
            }
            VStack(alignment: .leading, spacing: InterfaceMetrics.value(mac: 10, mobile: 14)) {
                if facts.isEmpty {
                    Text(loaded
                        ? "Nothing yet. The bot remembers who you are and what you work on as you chat."
                        : "Loading…")
                        .font(InterfaceMetrics.secondary)
                        .foregroundStyle(Palette.secondary)
                }
                ForEach(facts) { fact in
                    HStack(alignment: .firstTextBaseline, spacing: 10) {
                        Image(systemName: fact.kind == "profile" ? "person" : "clock")
                            .font(.caption)
                            .foregroundStyle(Palette.tertiary)
                            .help(fact.kind == "profile" ? "About you" : "History")
                        Text(fact.content)
                            .foregroundStyle(Palette.text)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        Button("Forget", systemImage: "xmark") { forget(fact) }
                            .labelStyle(.iconOnly)
                            .buttonStyle(.plain)
                            .font(.caption)
                            .foregroundStyle(Palette.tertiary)
                            .help("Forget")
                    }
                }
            }
            .padding(.horizontal, InterfaceMetrics.value(mac: 12, mobile: 18))
            .padding(.vertical, InterfaceMetrics.value(mac: 12, mobile: 16))
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        }
        .task(id: botId) { await load() }
        .codyncDialog("Forget everything this bot remembers?", isPresented: $confirmClear) {
            [DialogAction("Forget everything", destructive: true) { clear() }]
        }
    }

    private func load() async {
        guard let client = model.client else { return }
        facts = (try? await client.memory(botId: botId).facts) ?? []
        loaded = true
    }

    private func forget(_ fact: MemoryFact) {
        withAnimation(Motion.layout) { facts.removeAll { $0.id == fact.id } }
        Task {
            try? await model.client?.forgetMemory(botId: botId, id: fact.id)
            await load()
        }
    }

    private func clear() {
        withAnimation(Motion.layout) { facts = [] }
        Task {
            try? await model.client?.clearMemory(botId: botId)
            await load()
        }
    }
}

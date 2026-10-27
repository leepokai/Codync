import CodyncKit
import Foundation
import os

private let log = Logger(subsystem: "com.pokai.Codync", category: "Call")

/// How long a tool call waits for the user's words that are still being written down.
private let hearingPatience: Duration = .seconds(2)

/// A call as the computer runs it (host/src/chat/calls): the voice's instructions, tools and transcript live there;
/// the engines only carry audio. One per call, so it outlives an engine switch.
@MainActor
final class CallLink {
    let model: BotStore
    let botId: String
    /// The call's id: what it sends the bot carries it.
    let call: String
    private let usesGoogleSearch: @MainActor () -> Bool
    private var lines: [CallLine] = []
    private var open = false
    /// The user's last words are still being written down (OpenAI transcribes them after the turn).
    private var hearing = false
    /// The computer gets what was said, the tool calls and the end one at a time, in order: a hand-off never
    /// overtakes the words it hands on.
    private var queue: Task<Void, Never>?

    init(model: BotStore, botId: String, call: String, googleSearch: @escaping @MainActor () -> Bool = { VoiceSettings.googleSearch }) {
        usesGoogleSearch = googleSearch
        self.model = model
        self.botId = botId
        self.call = call
    }

    private func client() throws -> HostClient {
        guard let client = model.client else { throw VoiceError("Not connected to \(model.hostName).") }
        return client
    }

    /// Starts the call on the computer (again, when a provider session is replaced): what the voice needs.
    func start(_ mode: String, provider: VoiceProvider? = nil, model chosen: String? = nil, voice: String? = nil) async throws -> CallSession {
        try await client().startCall(botId: botId, call: call, mode: mode, provider: provider, model: chosen, voice: voice,
                                     language: Locale.preferredLanguages.first,
                                     googleSearch: mode == "realtime" && provider == .gemini && usesGoogleSearch())
    }

    /// One of the voice's tool calls, run on the computer with what was said that it hasn't got yet; its answer is
    /// JSON. A failure is the answer.
    func tool(_ name: String, arguments: Data) async -> Data {
        let deadline = ContinuousClock.now + hearingPatience
        while hearing, ContinuousClock.now < deadline { try? await Task.sleep(for: .milliseconds(100)) }
        return await serial { [self] in
            let said = drain(withOpen: true)
            do {
                return try await client().callTool(call: call, name: name, arguments: arguments, lines: said)
            } catch {
                restore(said)
                return (try? JSONSerialization.data(withJSONObject: ["error": error.localizedDescription])) ?? Data("{}".utf8)
            }
        }.value
    }

    /// A piece of what `from` (`user` or `voice`) said; the pieces of one turn join into one line.
    func add(_ from: String, _ piece: String) {
        if from == "user" { hearing = false }
        guard !piece.isEmpty else { return }
        if open, let last = lines.indices.last, lines[last].from == from {
            lines[last].text += piece
        } else {
            endTurn()
            lines.append(CallLine(from: from, text: piece, at: Int64(Date.now.timeIntervalSince1970 * 1000), id: UUID().uuidString))
            open = true
        }
    }

    /// The user stopped talking and their words are being written down (`true`), or that failed (`false`): a tool
    /// call waits a moment for them.
    func transcribing(_ on: Bool) {
        hearing = on
    }

    /// The turn is over: what was said goes to the computer, and the next piece starts a line.
    func endTurn() {
        open = false
        serial { [self] in
            let said = drain(withOpen: false)
            guard !said.isEmpty else { return }
            do { try await client().callLines(call: call, lines: said) } catch {
                restore(said)
                log.error("call lines: \(error.localizedDescription, privacy: .public)")
            }
        }
    }

    /// How a final reply of the bot reaches the voice.
    func reply(_ text: String) -> String {
        "[\(model.bots[botId]?.name ?? "Bot") replied] \(SpokenText.from(text))"
    }

    func notice(_ text: String) -> String { "[Notice] \(text)" }

    static let progressTag = "[Progress]"

    /// What the bot is doing, as the voice's instructions name it.
    func progress(_ text: String) -> String { "\(Self.progressTag) \(text)" }

    /// The call is over: its line in the chat, with what's left of the transcript.
    func end(seconds: Int) {
        model.beginCallEnd()
        serial { [self] in
            await model.endCall(botId, seconds: seconds, call: call, lines: drain(withOpen: true))
            model.finishCallEnd()
        }
    }

    /// The lines finished so far (`withOpen`: and the one still being said), out of the buffer: trimmed, Chinese in
    /// the reader's script.
    private func drain(withOpen: Bool) -> [CallLine] {
        let count = open && !withOpen ? lines.count - 1 : lines.count
        if withOpen { open = false }
        let taken = lines.prefix(max(count, 0))
        lines.removeFirst(taken.count)
        return taken.compactMap { line in
            let text = VoiceSettings.inReadersScript(line.text.trimmingCharacters(in: .whitespacesAndNewlines))
            return text.isEmpty ? nil : CallLine(from: line.from, text: text, at: line.at, id: line.id)
        }
    }

    /// Lines the computer didn't get go out with the next request.
    private func restore(_ said: [CallLine]) {
        lines.insert(contentsOf: said, at: 0)
    }

    @discardableResult
    private func serial<T: Sendable>(_ op: @escaping @MainActor () async -> T) -> Task<T, Never> {
        let previous = queue
        let task = Task { @MainActor in
            await previous?.value
            return await op()
        }
        queue = Task { _ = await task.value }
        return task
    }
}

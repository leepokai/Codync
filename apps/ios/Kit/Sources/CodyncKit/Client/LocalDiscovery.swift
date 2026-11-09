import Foundation
import Network

/// Finds computers in no account on the local network (docs/plans/optional-sign-in.md). They have no relay to tell a
/// phone their new address, so they advertise it over Bonjour. Nothing found here is trusted: the phone dials the
/// address and the handshake checks the computer's pinned key.
public actor LocalDiscovery {
    public static let shared = LocalDiscovery()

    /// Also in `NSBonjourServices`, and in the host's `remote/discovery.rs`.
    static let service = "_codync._tcp"

    private var browser: NWBrowser?
    private var found: [ComputerID: [String]] = [:]

    /// The addresses `id` advertises right now. The first call starts browsing, which asks for the Local Network
    /// permission a direct connection needs anyway.
    public func urls(for id: ComputerID) -> [String] {
        if browser == nil { start() }
        return found[id] ?? []
    }

    private func start() {
        let browser = NWBrowser(for: .bonjourWithTXTRecord(type: Self.service, domain: nil), using: .tcp)
        browser.browseResultsChangedHandler = { results, _ in
            let entries = results.compactMap { result -> (id: ComputerID, urls: [String])? in
                guard case let .bonjour(txt) = result.metadata else { return nil }
                return Self.entry(id: txt["id"], urls: txt["urls"])
            }
            Task { await self.update(entries) }
        }
        browser.stateUpdateHandler = { state in
            if case .failed = state { Task { await self.stop() } }
        }
        browser.start(queue: .global(qos: .utility))
        self.browser = browser
    }

    private func stop() {
        browser?.cancel()
        browser = nil
    }

    private func update(_ entries: [(id: ComputerID, urls: [String])]) {
        found = Dictionary(entries.map { ($0.id, $0.urls) }, uniquingKeysWith: { a, _ in a })
    }

    /// One advertised record: the computer's ID and its comma-separated addresses, kept only if they're valid.
    nonisolated static func entry(id: String?, urls: String?) -> (id: ComputerID, urls: [String])? {
        guard let id, !id.isEmpty, let urls else { return nil }
        let valid = Pairing.directURLs(urls.split(separator: ",").map(String.init))
        return valid.isEmpty ? nil : (id, valid)
    }
}

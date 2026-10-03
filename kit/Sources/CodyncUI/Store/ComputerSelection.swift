import CodyncKit

/// An empty exclusion list means all computers, including newly paired computers.
public struct ComputerSelection: Equatable, Sendable {
    public let all: [ComputerID]
    private let hidden: Set<ComputerID>

    public init(all: [ComputerID], hidden: String) {
        self.all = all
        self.hidden = Set(hidden.split(separator: ",").map(String.init))
    }

    public var shown: [ComputerID] {
        let visible = all.filter { !hidden.contains($0) }
        return visible.isEmpty ? all : visible
    }

    public func toggling(_ id: ComputerID) -> String {
        guard all.contains(id) else { return encoded(Set(all).subtracting(shown)) }
        var visible = Set(shown)
        if visible.contains(id) {
            if visible.count > 1 { visible.remove(id) }
        } else {
            visible.insert(id)
        }
        return encoded(Set(all).subtracting(visible))
    }

    public func only(_ id: ComputerID) -> String {
        guard all.contains(id) else { return "" }
        return encoded(Set(all).subtracting([id]))
    }

    private func encoded(_ ids: Set<ComputerID>) -> String { ids.sorted().joined(separator: ",") }
}

/// A connection in progress before the computer has attached to the account.
public struct ComputerConnectionProgress {
    public let computerId: ComputerID?
    public let name: String
    public let detail: String

    public init(computerId: ComputerID?, name: String, detail: String) {
        self.computerId = computerId
        self.name = name
        self.detail = detail
    }
}

struct ConnectionSummary {
    let connections: [BotStore.Connection]
    private var additionalConnecting = 0

    init(connections: [BotStore.Connection]) { self.connections = connections }

    init(computers: [ComputerID: BotStore.Connection], progress: [ComputerConnectionProgress]) {
        var connections = computers
        var additionalConnecting = 0
        for pending in progress {
            if let id = pending.computerId {
                // An existing route may already be online while SSH connects.
                if connections[id] != .online { connections[id] = .connecting }
            } else {
                additionalConnecting += 1
            }
        }
        self.connections = Array(connections.values)
        self.additionalConnecting = additionalConnecting
    }

    var progressText: String {
        let connecting = connections.filter { $0 == .connecting }.count + additionalConnecting
        guard connecting > 0 else { return text }
        let online = connections.filter { $0 == .online }.count
        return online > 0 ? "\(online) connected · \(connecting) connecting…" : "\(connecting) connecting…"
    }

    var text: String {
        guard !connections.isEmpty else { return "Computers" }
        let online = connections.filter { $0 == .online }.count
        if online == connections.count { return "\(online) connected" }
        if online > 0 { return "\(online)/\(connections.count) connected" }
        if connections.contains(.connecting) { return "Connecting…" }
        if connections.allSatisfy({ if case .unauthorized = $0 { true } else { false } }) { return "No access" }
        if connections.allSatisfy({ $0 == .unpaired }) { return "Not paired" }
        return "\(connections.count) offline"
    }
}

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

    private func encoded(_ ids: Set<ComputerID>) -> String { ids.sorted().joined(separator: ",") }
}

struct ConnectionSummary {
    let connections: [BotStore.Connection]

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

import Foundation

/// A program a bot keeps running on the computer: the host runs it (`managed`), or the bot left it running through
/// its own shell. What each state shows and offers is decided here, the same on Android (`DevServer.kt`).
public struct DevServer: Codable, Identifiable, Hashable, Sendable {
    public enum Status: String, Codable, Sendable {
        case starting, running, stopped, failed

        /// A status a newer host adds reads as stopped, rather than losing the bot's servers.
        public init(from decoder: Decoder) throws {
            self = Status(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .stopped
        }
    }

    public struct Port: Codable, Hashable, Sendable {
        public var port: Int
        /// Works on the computer running it.
        public var local: String
        /// Work from other devices; empty when it listens only on that computer.
        public var remote: [String]
    }

    /// What the host can be asked to do with a server. Clients name it and never send a command.
    public enum Action: String, Sendable {
        case stop = "devServerStop", restart = "devServerRestart", remove = "devServerRemove"
    }

    /// What tapping its row does.
    public enum Tap: Equatable, Sendable {
        case open(String), logs
    }

    /// A line of its menu.
    public enum MenuRow: Equatable, Sendable {
        case onlyOnComputer, open, openAddress(String), copyAddress, logs, restart, start, startedByShell, stop, remove
    }

    public var id: String
    public var name: String
    public var command: String
    public var folder: String?
    public var status: Status
    public var ports: [Port]
    public var startedAt: Int64?
    public var exitCode: Int?
    public var managed: Bool
}

public extension DevServer {
    /// Where a phone can reach it while it runs, best first: the first port that has any.
    var remoteAddresses: [String] {
        status == .running ? ports.first { !$0.remote.isEmpty }?.remote ?? [] : []
    }

    /// The best of them; nil when it listens only on its computer.
    var remoteAddress: String? { remoteAddresses.first }

    /// An address as a menu row shows it.
    static func addressTitle(_ address: String) -> String {
        String(address.trimmingPrefix("http://"))
    }

    /// Its row's value: its ports, or a phrase to look up.
    var rowValue: String {
        switch status {
        case .starting: "Starting"
        case .stopped: "Stopped"
        case .failed: "Failed to start"
        case .running: ports.isEmpty ? "Running" : ports.map { ":\($0.port)" }.joined(separator: " ")
        }
    }

    /// Open what can be reached, or read the log of what has no port (a watch build, or one that ended).
    var tap: Tap? {
        if let remoteAddress { return .open(remoteAddress) }
        let listens = status == .running && !ports.isEmpty
        return managed && !listens ? .logs : nil
    }

    /// Only what applies. A server the bot started through its shell can only be opened or stopped. A tap opens the first
    /// address, which this phone may not reach (a computer's Tailscale address, without Tailscale here), so when there are
    /// more each one is a row.
    var menuRows: [MenuRow] {
        let live = status == .running || status == .starting
        var rows: [MenuRow] = []
        if status == .running, !ports.isEmpty, remoteAddress == nil { rows.append(.onlyOnComputer) }
        if remoteAddresses.count > 1 {
            rows += remoteAddresses.map(MenuRow.openAddress)
        } else if remoteAddress != nil {
            rows.append(.open)
        }
        if remoteAddress != nil { rows.append(.copyAddress) }
        if managed {
            rows.append(.logs)
            if status != .starting { rows.append(live ? .restart : .start) }
        } else {
            rows.append(.startedByShell)
        }
        if live { rows.append(.stop) } else if managed { rows.append(.remove) }
        return rows
    }
}

/// What the chat's title shows of a bot's servers: nothing until one starts, or once they've all stopped.
public enum DevServerSummary: Equatable, Sendable {
    case none, starting, failed
    case running(String)

    public init(_ servers: [DevServer]) {
        let running = servers.filter { $0.status == .running }
        if let first = running.first(where: { !$0.ports.isEmpty }) ?? running.first {
            let label = first.ports.first.map { ":\($0.port)" } ?? first.name
            self = .running(running.count > 1 ? "\(label) +\(running.count - 1)" : label)
        } else if servers.contains(where: { $0.status == .starting }) {
            self = .starting
        } else if servers.contains(where: { $0.status == .failed }) {
            self = .failed
        } else {
            self = .none
        }
    }
}

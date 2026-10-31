import CodyncKit
import Foundation

/// This device's section order per computer. The empty name stands for the unsectioned Bots group.
public struct RosterSectionOrder: Codable, Sendable {
    private var computers: [ComputerID: [String]] = [:]

    public init(data: Data) {
        self = (try? JSONDecoder().decode(Self.self, from: data)) ?? Self()
    }

    private init() {}

    public var data: Data { (try? JSONEncoder().encode(self)) ?? Data() }

    public func saved(on computer: ComputerID) -> [String] { computers[computer] ?? [] }

    public mutating func set(_ names: [String], on computer: ComputerID) {
        computers[computer] = names
    }

    public mutating func rename(_ from: String, to: String?, on computer: ComputerID) {
        guard from != to else { return }
        let order = computers[computer] ?? []
        computers[computer] = order.compactMap { name in
            guard name == from else { return name }
            return to.flatMap { order.contains($0) ? nil : $0 }
        }
    }
}

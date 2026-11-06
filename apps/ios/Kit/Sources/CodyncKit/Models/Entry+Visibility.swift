import Foundation

public extension Entry {
    /// Internal work stays in the mirror for revision ordering; the host promotes useful output explicitly.
    var isInternal: Bool { data.`internal` == true || (kind == "user" && data.from == "proactive") }
}

public extension Array where Element == Entry {
    /// Used for both live entries and restored mirrors, before they reach any presentation.
    func visibleEntries() -> [Entry] { filter { !$0.isInternal } }
}

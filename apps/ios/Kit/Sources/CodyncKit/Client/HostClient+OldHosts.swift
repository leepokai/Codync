import Foundation

// Hosts before 2.51.0: goes when minHost reaches 2.51.0.
//
// A host before 2.51.0 says `item` for a task and `task` for a thread. Everything that knows those names is here:
// `HostClient.call` sends through `OldHostWire.request`, and the decoders of `Bot`, `WorkTask`, `Entry` and the
// `threadEntry` event read the old names through `OldHostWire.value` and `OldHostWire.EntryRefs`. The store, the
// models and the views know only the new names.

public extension HostClient {
    /// The first host that says thread and task.
    static let threadsHost = "2.51.0"

    /// Whether a thread's transcript is asked for under its new name. A host whose version isn't known yet (no
    /// hello) or doesn't read counts as new.
    var speaksThreads: Bool {
        hostVersion.map { !AppVersion.isBelow($0, Self.threadsHost) } ?? true
    }
}

enum OldHostWire {
    /// Arguments an older host knows under another name, by method.
    private static let taskMethods: Set<String> = ["send", "steer", "unqueue", "workFirst", "workCutIn", "dropWork", "stopWork", "retryWork"]
    private static let botMethods: Set<String> = ["createBot", "updateBot"]

    /// A call as either host takes it: its method and body. What names a task or a bot's thread settings carries
    /// both names whatever the host's version, so a version that is missing or left from the last connection never
    /// sends a thread's message to the chat: each host reads the name it knows and ignores the other. Only
    /// `threadEntries` goes by the version (`speaksThreads`): a wrong guess there is a read that fails. A call that
    /// names no task or thread goes as it is.
    static func request(_ method: String, _ body: Data, speaksThreads: Bool) -> (method: String, body: Data) {
        let oldNames: [String: String]
        let transcript = method == "threadEntries"
        if taskMethods.contains(method) {
            oldNames = ["taskId": "itemId"]
        } else if botMethods.contains(method) {
            oldNames = ["maxThreads": "maxAgents", "threadWorktrees": "taskWorktrees"]
        } else if transcript, !speaksThreads {
            // Its answer's `task` in place of `thread` isn't read: only `entries` is.
            oldNames = ["threadId": "taskId"]
        } else {
            return (method, body)
        }
        let sent = transcript ? "taskEntries" : method
        guard var fields = (try? JSONSerialization.jsonObject(with: body)) as? [String: Any] else { return (sent, body) }
        for (new, old) in oldNames {
            guard let value = fields[new] else { continue }
            fields[old] = value
            if transcript { fields[new] = nil }
        }
        return (sent, (try? JSONSerialization.data(withJSONObject: fields)) ?? body)
    }

    private struct Key: CodingKey {
        var stringValue: String
        var intValue: Int? { nil }
        init(_ name: String) { stringValue = name }
        init?(stringValue: String) { self.stringValue = stringValue }
        init?(intValue: Int) { nil }
    }

    /// What an older host sent under `name`; nil when it isn't there or doesn't read.
    static func value<T: Decodable>(_ type: T.Type, _ name: String, from decoder: Decoder) -> T? {
        guard let fields = try? decoder.container(keyedBy: Key.self) else { return nil }
        return try? fields.decodeIfPresent(type, forKey: Key(name))
    }

    /// An entry's `data` as an older host wrote it: `task` is the thread (`{id, title}`) and `item` the task's id.
    struct EntryRefs: Decodable {
        var task: ThreadRef?
        var item: TaskRef?
    }
}

extension EntryData {
    /// Takes the thread and the task an older host named: its `task` as an object is the thread, its `item` the
    /// task. The new shape (`task` as the task's id) is left as read.
    mutating func adopt(_ old: OldHostWire.EntryRefs) {
        if let named = old.task, !(named.id.isEmpty && named.title.isEmpty) {
            thread = named
            task = nil
        }
        if let item = old.item, !item.id.isEmpty { task = item }
    }
}

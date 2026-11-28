import Foundation

/// A task's thread as a message is addressed to it: the bot and its task (what the host is told), and the id of
/// the thread, whose transcript holds its entries. The rules here read the same on Android and the desktop.
public struct WorkThread: Hashable, Sendable {
    public var botId: String
    public var taskId: String
    public var threadId: String

    public init(botId: String, taskId: String, threadId: String) {
        self.botId = botId
        self.taskId = taskId
        self.threadId = threadId
    }

    /// Where the composer keeps what is typed for it, apart from the bot's chat and gone with the bot. `item` is
    /// what a task was called when drafts were first kept: the key stays, so a draft written before 2.51.0 is
    /// still there.
    public var draftKey: String { "\(botId)/item/\(taskId)" }
}

/// Where a task's thread works, in one quiet line: its branch when it has a worktree of its own, else its folder.
public struct WhereLine: Equatable, Sendable {
    public enum Kind: Sendable {
        case folder, branch

        public var symbol: String { self == .folder ? "folder" : "arrow.triangle.branch" }
    }

    public var kind: Kind
    public var text: String
    /// The whole path on the computer, which a tap copies; nil when the host didn't say.
    public var path: String?

    /// `path` with the computer's `home` folder as `~`, on either kind of separator; unchanged outside the home
    /// folder or without one.
    public static func homeRelative(_ path: String, home: String) -> String {
        let separators: Set<Character> = ["/", "\\"]
        var root = Substring(home)
        while let last = root.last, separators.contains(last) { root.removeLast() }
        guard !root.isEmpty, path.hasPrefix(root) else { return path }
        let rest = path.dropFirst(root.count)
        return rest.first.map(separators.contains) ?? true ? "~\(rest)" : path
    }
}

public extension WorkTask {
    /// Why its thread is only read (a phrase to look up): a routine's run and another bot's request have an owner
    /// for their result already. Nil for the bot's own task.
    var readOnlyLine: String? {
        switch source?.kind {
        case .routine: "Read only: a routine's run"
        case .request: "Read only: another bot's request"
        case nil: nil
        }
    }

    /// Nil before a thread took it. A host that names no folder still shows a worktree's branch. `home`: the
    /// computer's home folder, which a folder shows as `~`; the folder Codync made for it reads "Own folder".
    func whereLine(home: String) -> WhereLine? {
        let path = wherePath
        if let worktree { return WhereLine(kind: .branch, text: worktree.branch, path: path) }
        if ownFolder, let path { return WhereLine(kind: .folder, text: L10n.tr("Own folder"), path: path) }
        return path.map { WhereLine(kind: .folder, text: WhereLine.homeRelative($0, home: home), path: $0) }
    }

    /// Its thread, once one has it.
    func workThread(of botId: String) -> WorkThread? {
        thread.map { WorkThread(botId: botId, taskId: id, threadId: $0) }
    }
}

public extension Bot {
    /// What its folder is to git, under the folder in Settings.
    enum GitState: Sendable {
        case repo, none, missing

        /// A phrase to look up.
        public var line: String {
            switch self {
            case .repo: "Git repository"
            case .none: "Not a git repository"
            case .missing: "Git isn't installed on this computer"
            }
        }
    }

    /// A host that doesn't say whether git is installed has it.
    var gitState: GitState {
        gitInstalled == false ? .missing : gitRepo ? .repo : .none
    }
}

public extension Entry {
    /// What a thread was told (its task, or what came later): the host wrote it, the user didn't type it.
    var isToldToThread: Bool { kind == "user" && data.from == "host" }

    /// The id of a message shown before the computer has it, by the nonce it was sent with.
    static func localId(_ nonce: String) -> String { "local-\(nonce)" }
}

/// A thread's transcript in two parts: what the host first told it (folded on top), and everything else in
/// order. What the host told it later (a task picked up again) stays where it happened.
public struct ThreadTranscript: Equatable, Sendable {
    public var told: Entry?
    public var rest: [Entry]

    /// While the thread works, the user's messages that wait above the composer (`queue(working:)`). What the host told
    /// it, the chat passing words on included, is never one of them: it stays quoted where it came.
    public func queue(working: Bool) -> [Entry] {
        rest.filter { !$0.isToldToThread }.queue(working: working)
    }

    /// `rest` without that queue.
    public func hidingQueue(working: Bool) -> [Entry] {
        working ? rest.filter { !$0.isQueued || $0.isToldToThread } : rest
    }
}

public extension Array where Element == Entry {
    /// A message the user took back is in neither part.
    var threadTranscript: ThreadTranscript {
        let shown = filter { $0.data.removed != true }
        let told = shown.first(where: \.isToldToThread)
        return ThreadTranscript(told: told, rest: shown.filter { $0.id != told?.id })
    }

    /// A thread's entries with what the thread `thread` put in the bot's `chat` (a file, a voice note, an artifact, a
    /// card, answered or not) where it came (`createdAt`), each kept in its order; at the same
    /// moment the thread's own first.
    func sharing(_ chat: [Entry], of thread: String) -> [Entry] {
        var shared = chat.filter { $0.data.thread?.id == thread }[...]
        var out: [Entry] = []
        for e in self {
            while let next = shared.first, next.createdAt < e.createdAt { out.append(shared.removeFirst()) }
            out.append(e)
        }
        return out + shared
    }
}

import CodyncKit
import SwiftUI

extension EnvironmentValues {
    /// The task's thread the views under it write to: the composer, its waiting messages and the user's bubbles
    /// address it instead of the bot's chat. Nil in a chat.
    @Entry var workThread: WorkThread?
}

/// One task's thread, a page pushed inside its bot's details under a header of our own (Back to details, its title,
/// where it stands and where it works, Stop): everything its thread did, with one quiet line where it put something in
/// the bot's chat (kept current while it works, following the newest like the chat), and the chat's composer for a
/// task that takes messages.
struct WorkThreadView: View {
    let botId: String
    /// The task as its row had it; the bot's own copy is shown while it carries one.
    let task: WorkTask
    /// Closes the details and shows the chat at an entry its thread put there; nil where that chat isn't under them.
    let openInChat: ((Entry) -> Void)?
    @Environment(BotStore.self) private var model
    @Environment(\.dismiss) private var dismiss
    /// The thread's transcript as the computer has it; nil while it loads.
    @State private var fetched: [Entry]?
    @State private var error: String?
    @State private var attempt = 0
    @State private var confirmingStop = false
    @State private var following = true
    @State private var toldOpen = false

    /// What reads the transcript again: Try again, a thread taking the task, the task ending, and the link coming back
    /// (its catch-up carries no thread entries).
    private struct Load: Hashable {
        var attempt: Int
        var thread: String?
        var live: Bool
        var online: Bool
    }

    var body: some View {
        let task = model.bots[botId]?.work.first { $0.id == self.task.id } ?? self.task
        let live = task.status.isLive
        let entries = (fetched ?? []).mergingThreadEntries(task.thread.flatMap { model.threadEntries[$0] } ?? []).threadTranscript
        ConversationList(rows: rows(entries, of: task), following: $following)
            .background(Palette.background)
            .safeAreaInset(edge: .top, spacing: 0) { header(task) }
            .safeAreaInset(edge: .bottom, spacing: 0) { footer(task, queued: entries.queue(working: live)) }
            .environment(\.workThread, task.workThread(of: botId))
            .hidesSystemNavigationBar()
            .task(id: Load(attempt: attempt, thread: task.thread, live: live, online: model.connection == .online)) { await load(task.thread) }
            .stopWorkDialog(isPresented: $confirmingStop) { model.stopWork(botId, task: task.id) }
            .tracked(.screenViewed(.thread))
    }

    /// Back to the details, the task's title over where it stands and where it works, and Stop while a thread has it.
    private func header(_ task: WorkTask) -> some View {
        HStack(spacing: 6) {
            IconButton("Back to details", systemImage: "chevron.left") { dismiss() }
            VStack(alignment: .leading, spacing: 2) {
                Text(task.title)
                    .font(.body.weight(.semibold))
                    .foregroundStyle(Palette.text)
                    .lineLimit(1)
                    .accessibilityAddTraits(.isHeader)
                ThreadStatusLine(task: task)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if task.status.isLive {
                IconButton("Stop", systemImage: "stop.fill") { confirmingStop = true }
            }
        }
        .padding(.leading, 10)
        .padding(.trailing, 12)
        .padding(.top, 12)
        .padding(.bottom, 6)
        .background(Palette.background)
    }

    /// What it was told, folded; then what its thread did and what the user wrote, without the messages that wait
    /// above the composer, and a line where it put something in the chat (a card, a file), which opens it there.
    private func rows(_ entries: ThreadTranscript, of task: WorkTask) -> [ConversationRow] {
        let live = task.status.isLive
        let own = entries.hidingQueue(working: live)
        let running = Entry.runningTurn(own, working: live)
        let shown = task.thread.map { own.sharing(model.chat(botId), of: $0) } ?? own
        var rows: [ConversationRow] = []
        if let told = entries.told {
            rows.append(ConversationRow("told") { ToldFold(entry: told, isExpanded: $toldOpen) })
        }
        if error != nil || fetched == nil || (shown.isEmpty && entries.told == nil) {
            rows.append(ConversationRow("state") { state.frame(maxWidth: .infinity).padding(.vertical, 14) })
        }
        rows += Entry.nested(shown).map { entry, depth in
            // The chat's entry stays in the chat, answered and downloaded there.
            if entry.botId == botId {
                return ConversationRow(entry.id) { SharedRow(entry: entry, open: openInChat.map { open in { open(entry) } }) }
            }
            return ConversationRow(ThreadRow.id(entry), isUserMessage: entry.kind == "user" && !entry.isToldToThread) {
                ThreadRow(entry: entry, unfolded: entry.turn == running).nested(depth)
            }
        }
        return rows
    }

    @ViewBuilder private var state: some View {
        if let error {
            VStack(spacing: 8) {
                Text(L10n.tr(error)).font(.footnote).foregroundStyle(Palette.danger)
                Button(L10n.tr("Try again")) { attempt += 1 }
                    .font(.footnote.weight(.medium))
                    .foregroundStyle(Palette.text)
                    .buttonStyle(PressScale())
            }
        } else if fetched == nil {
            Spinner(size: 14).accessibilityLabel(L10n.tr("Loading"))
        } else {
            Text(L10n.tr("Nothing yet.")).font(.footnote).foregroundStyle(Palette.tertiary)
        }
    }

    /// The composer for a task that takes messages; for a routine's run or another bot's request, why it doesn't.
    @ViewBuilder private func footer(_ task: WorkTask, queued: [Entry]) -> some View {
        if task.talk, task.thread != nil {
            Composer(botId: botId, replyingTo: .constant(nil), queued: queued)
                .disabled(model.isOffline)
        } else if let line = task.readOnlyLine {
            Text(L10n.tr(line))
                .font(.footnote)
                .foregroundStyle(Palette.tertiary)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 14)
                .background(Palette.background)
        }
    }

    private func load(_ thread: String?) async {
        error = nil
        guard let thread else {
            fetched = []
            return
        }
        do {
            fetched = try await model.fetchThreadEntries(botId, thread: thread)
        } catch is CancellationError {
        } catch {
            self.error = error.localizedDescription
        }
    }
}

/// Under the title: the status and how long, then where the thread works. A tap on that copies the whole path.
private struct ThreadStatusLine: View {
    let task: WorkTask
    @Environment(BotStore.self) private var model
    @State private var copied = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        HStack(spacing: 12) {
            HStack(spacing: 6) {
                WorkMark(status: task.status)
                Text(L10n.tr(task.status.word)).font(.footnote).foregroundStyle(Palette.secondary)
                WorkElapsed(task: task)
            }
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("thread-status")
            if let place = task.whereLine(home: model.hello?.home ?? "") { location(place) }
        }
    }

    @ViewBuilder private func location(_ place: WhereLine) -> some View {
        if let path = place.path {
            Button {
                Pasteboard.copy(path)
                withAnimation(Motion.reduced(Motion.hover, reduceMotion)) { copied = true }
            } label: {
                if copied {
                    Label(L10n.tr("Path copied"), systemImage: "checkmark").font(.footnote).foregroundStyle(Palette.tertiary)
                } else {
                    WhereLabel(place: place)
                }
            }
            .buttonStyle(PressScale())
            .accessibilityHint(L10n.tr("Copy path"))
            .accessibilityIdentifier("thread-where")
            // The words give the place back after a moment.
            .task(id: copied) {
                guard copied, (try? await Task.sleep(for: .seconds(1.5))) != nil else { return }
                withAnimation(Motion.reduced(Motion.hover, reduceMotion)) { copied = false }
            }
        } else {
            WhereLabel(place: place)
        }
    }
}

/// What the host told the thread, folded on top of it.
private struct ToldFold: View {
    let entry: Entry
    @Binding var isExpanded: Bool

    var body: some View {
        Disclosure(isExpanded: $isExpanded) {
            ToldText(entry: entry).transition(.opacity)
        } label: {
            Text(L10n.tr("What it was told")).font(.footnote.weight(.medium)).foregroundStyle(Palette.secondary)
        }
        .padding(.vertical, 6)
    }
}

/// The host's words to the thread: quiet, and not a bubble, since the user didn't type them.
private struct ToldText: View {
    let entry: Entry

    var body: some View {
        Text(entry.data.text ?? "")
            .font(.subheadline)
            .foregroundStyle(Palette.secondary)
            .textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// What the thread put in the bot's chat, as one quiet line (`Entry.sharedLine`); a tap shows it in the chat.
private struct SharedRow: View {
    let entry: Entry
    let open: (() -> Void)?

    var body: some View {
        if let line = entry.sharedLine() {
            Button { open?() } label: {
                Label(line.text, systemImage: line.symbol)
                    .font(.footnote)
                    .foregroundStyle(Palette.secondary)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(.rect)
            }
            .buttonStyle(PressScale())
            .disabled(open == nil)
            .accessibilityHint(open == nil ? "" : L10n.tr("Show in the chat"))
            .padding(.vertical, 4)
        }
    }
}

/// One entry of a thread, drawn by the chat's own pieces: the user's bubble, the agent's final reply, and a step (a
/// tool call, thinking, a plan, what the agent says on the way) as the chat's working steps show it.
private struct ThreadRow: View {
    let entry: Entry
    /// The running turn's thoughts start unfolded.
    let unfolded: Bool

    /// A message sent from here keeps its row when the computer's copy replaces it.
    static func id(_ entry: Entry) -> String {
        if entry.kind == "user", let nonce = entry.data.clientNonce, !nonce.isEmpty { return "user-\(nonce)" }
        return entry.id
    }

    var body: some View {
        switch entry.kind {
        case "user" where entry.isToldToThread:
            // Told again later (a task picked up): a quoted block where it happened.
            ToldText(entry: entry)
                .padding(.leading, 12)
                .overlay(alignment: .leading) { Capsule().fill(Palette.tertiary).frame(width: 2) }
                .padding(.vertical, 8)
        case "user": UserBubble(entry: entry).padding(.top, 8)
        case "agent" where entry.isThreadMessage: AgentBubble(entry: entry, openTrace: {}).padding(.top, 8)
        default: TraceRow(entry: entry, unfolded: unfolded).padding(.vertical, 5)
        }
    }
}

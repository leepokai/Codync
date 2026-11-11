import CodyncKit
import SwiftUI

/// The messages waiting behind the running turn, as Android `QueuedStrip` and the desktop `QueuedStrip`: the next
/// one over the message box with Edit, Steer and Delete, and the whole queue in a sheet. Steer hands one to the agent
/// now (host `steer`) instead of after the turn; Edit and Delete only change what waits.
struct QueuedMessages: View {
    let botId: String
    /// The side conversation (`doc:<id>`) the composer writes to; nil in the chat itself.
    var thread: String?
    /// The agent is answering this lane, so Steer can reach it.
    let working: Bool
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var showingQueue = false
    @State private var editing: Entry?

    private var queued: [Entry] {
        model.chat(botId).filter { $0.threadId == thread && $0.kind == "user" && $0.data.status == "queued" }
    }

    var body: some View {
        let list = queued
        if let next = list.first {
            VStack(alignment: .leading, spacing: 2) {
                Text(L10n.tr("Waiting to be sent")).font(.caption2).foregroundStyle(Palette.tertiary)
                Text(preview(next)).font(.subheadline).foregroundStyle(Palette.text).lineLimit(2)
                HStack(spacing: 2) {
                    QueuePill("Edit", systemImage: "pencil") { editing = next }
                    if working {
                        QueuePill("Steer it into the reply now", systemImage: "arrow.turn.down.right", tint: Palette.accent) {
                            model.steerQueued(next)
                        }
                    }
                    QueuePill("Delete", systemImage: "trash") { model.deleteQueued(next) }
                }
                .padding(.top, 2)
                if list.count > 1 {
                    let rest = list.count - 1
                    Button { showingQueue = true } label: {
                        HStack(spacing: 5) {
                            Image(systemName: "rectangle.stack.fill").font(.system(size: 11, weight: .semibold))
                            Text(rest == 1 ? L10n.tr("1 more waiting") : L10n.tr("{count} more waiting").replacingOccurrences(of: "{count}", with: "\(rest)"))
                                .font(.caption)
                            Spacer(minLength: 0)
                            Image(systemName: "chevron.right").font(.system(size: 9, weight: .semibold))
                                .foregroundStyle(Palette.tertiary)
                        }
                        .foregroundStyle(Palette.secondary)
                        .padding(.top, 6)
                        .contentShape(.rect)
                    }
                    .buttonStyle(PressScale())
                    .overlay(alignment: .top) { Divider() }
                }
            }
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: 18))
            .padding(.horizontal, 16)
            .transition(.move(edge: .bottom).combined(with: .opacity))
            .animation(Motion.reduced(Motion.layout, reduceMotion), value: list.map(\.id))
            .codyncSheet(isPresented: $showingQueue) { queueSheet }
            .codyncSheet(item: $editing) { entry in EditQueuedSheet(entry: entry) }
        }
    }

    /// What a waiting message says: its text, or the names of what it carries.
    private func preview(_ entry: Entry) -> String {
        let text = entry.replyQuote.text.trimmingCharacters(in: .whitespacesAndNewlines)
        return text.isEmpty ? L10n.tr("Attachment") : text
    }

    private var queueSheet: some View {
        VStack(spacing: 0) {
            ModalHeader("Message queue")
            Text(L10n.tr("Steer sends a message into the reply now; Edit and Delete only change what waits."))
                .font(.caption)
                .foregroundStyle(Palette.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 20)
                .padding(.bottom, 8)
            ScrollView {
                LazyVStack(spacing: 0) {
                    ForEach(queued) { entry in
                        VStack(alignment: .leading, spacing: 6) {
                            Text(preview(entry)).font(.body).foregroundStyle(Palette.text).lineLimit(4)
                            HStack(spacing: 2) {
                                QueuePill("Edit", systemImage: "pencil") { editing = entry }
                                if working {
                                    QueuePill("Steer it into the reply now", systemImage: "arrow.turn.down.right", tint: Palette.accent) {
                                        model.steerQueued(entry)
                                        showingQueue = false
                                    }
                                }
                                QueuePill("Delete", systemImage: "trash") { model.deleteQueued(entry) }
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 20)
                        .padding(.vertical, 10)
                        Divider().padding(.horizontal, 20)
                    }
                }
            }
        }
        .background(Palette.background)
        .presentationDetents([.medium, .large])
    }
}

/// One named action on a waiting message.
private struct QueuePill: View {
    let title: String
    let systemImage: String
    var tint: Color = Palette.secondary
    let action: () -> Void

    init(_ title: String, systemImage: String, tint: Color = Palette.secondary, action: @escaping () -> Void) {
        self.title = title
        self.systemImage = systemImage
        self.tint = tint
        self.action = action
    }

    var body: some View {
        Button(action: action) {
            HStack(spacing: 5) {
                Image(systemName: systemImage).font(.system(size: 11, weight: .semibold))
                Text(L10n.tr(title)).font(.caption.weight(.semibold))
            }
            .foregroundStyle(tint)
            .padding(.horizontal, 9)
            .padding(.vertical, 5)
            .contentShape(.rect)
        }
        .buttonStyle(PressScale())
    }
}

/// A waiting message rewritten before it goes out; the agent reads it when its turn comes.
private struct EditQueuedSheet: View {
    let entry: Entry
    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss
    @State private var text: String
    @State private var saving = false
    @FocusState private var focused: Bool

    init(entry: Entry) {
        self.entry = entry
        _text = State(initialValue: entry.data.text ?? "")
    }

    private var trimmed: String { text.trimmingCharacters(in: .whitespacesAndNewlines) }

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader("Edit message") {
                if saving {
                    Spinner()
                } else {
                    IconButton("Save", systemImage: "checkmark", action: save)
                        .disabled(trimmed.isEmpty || trimmed == entry.data.text)
                }
            }
            CardForm {
                CardSection {
                    TextField(L10n.tr("Message"), text: $text, axis: .vertical)
                        .lineLimit(2...6)
                        .focused($focused)
                }
            }
            .textFieldStyle(.plain)
        }
        .background(Palette.background)
        .presentationDetents([.medium])
        .onAppear { focused = true }
    }

    private func save() {
        guard !saving, !trimmed.isEmpty else { return }
        saving = true
        Task {
            if await model.editQueued(entry, text: trimmed) { dismiss() }
            saving = false
        }
/// The user's messages waiting behind the bot's work, above the composer: each one line, sent now into the running
/// turn (`steer`) or taken back. An older computer has neither action, so the rows only show.
struct QueuedMessages: View {
    let entries: [Entry]
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        // A long queue scrolls, so every row stays in reach and the chat keeps its room.
        ScrollViewReader { proxy in
            CappedHeight(max: 190) {
                ScrollView {
                    VStack(spacing: 0) {
                        ForEach(entries) { entry in
                            row(entry).id(entry.id).transition(.move(edge: .bottom).combined(with: .opacity))
                        }
                    }
                    .padding(.vertical, 2)
                }
                .scrollIndicators(.hidden)
                .scrollBounceBehavior(.basedOnSize)
            }
            // It stays where it is, the oldest (next to go) in view; a message sent from here shows that it arrived.
            .onChange(of: entries.last?.id) {
                guard let newest = entries.last, let nonce = newest.data.clientNonce, nonce == model.lastSentNonce else { return }
                withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { proxy.scrollTo(newest.id, anchor: .bottom) }
            }
        }
        .background(Palette.surface)
        .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: entries.map(\.id))
    }

    private func row(_ entry: Entry) -> some View {
        HStack(spacing: 2) {
            Text(MessagePreview.text(entry.replyQuote.text))
                .font(.subheadline)
                .foregroundStyle(Palette.text)
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.vertical, model.supportsQueueActions ? 0 : 9)
            if model.supportsQueueActions {
                Group {
                    IconButton("Send now", systemImage: "arrow.turn.down.right") { model.steer(entry) }
                    IconButton("Remove", systemImage: "trash") { model.unqueue(entry) }
                }
                .disabled(model.answering[entry.id] != nil)
            }
        }
        .padding(.leading, 14)
        .padding(.trailing, 4)
    }
}

import CodyncKit
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

/// The message box under a chat: send, or stop while the bot (or the group) is working
/// there. A leading `/` lists commands (a group's act on its bots); in a group, typing `@`
/// lists its bots. Under a task's thread (`workThread`) it takes text, and files where the computer takes them: no
/// dictation, commands or call. A new thread's (`start`) takes text only, with what the thread runs on in its row.
struct Composer: View {
    let botId: String
    /// The message the next send quotes, shown above the field until sent or cancelled.
    @Binding var replyingTo: ReplyQuote?
    /// The user's messages waiting behind the bot's work, shown above the box (`QueuedMessages`).
    var queued: [Entry] = []
    /// Starts a voice call; while the box is empty it takes the send button's place.
    var onCall: (() -> Void)?
    /// Interrupts the bot while it is speaking in a voice call.
    var onInterrupt: (() -> Void)?
    /// An artifact's viewer: every message is about the artifact (a reply to its card), from a draft of its own.
    var artifact: Artifact?
    /// Under a task's thread: Stop while it runs, which asks before it stops the thread.
    var stopThread: (() -> Void)?
    /// A new thread's box: its own draft, what it runs on in the row, and its send.
    var start: ComposerStart?
    @Environment(BotStore.self) private var model
    @Environment(\.workThread) private var thread
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// The field owns its text; the store only keeps a copy. A field bound straight to the
    /// store loses a hardware keyboard's IME composition (Zhuyin, Pinyin, Kana) on iPad.
    @State private var input = ComposerText()
    /// Programmatic edits must discard indices into the previous string. Keyboard edits
    /// bind directly to `input.text` so SwiftUI can maintain selection and IME composition.
    private var draft: String {
        get { input.text }
        nonmutating set { input.replace(with: newValue) }
    }
    /// Files going out with the next message.
    @State private var files: [OutgoingFile] = []
    @State private var pickingFiles = false
    @State private var pickingPhotos = false
    @State private var photoItems: [PhotosPickerItem] = []
    @FocusState private var focused: Bool
    @State private var dictation = Dictation()
    /// A new thread's send while it is on its way, cancelled when the box goes.
    @State private var starting: Task<Void, Never>?

    private var bot: Bot? { model.bots[botId] }
    private var draftKey: String { start?.draftKey ?? thread?.draftKey ?? artifact.map(BotStore.artifactDraftKey) ?? botId }
    /// The task a thread's box writes to.
    private var threadTask: WorkTask? { thread.flatMap { thread in bot?.work.first { $0.id == thread.taskId } } }
    /// The turn its Stop is for: the chat's own, or the thread's task while it runs.
    private var working: Bool { start != nil ? false : thread == nil ? bot?.isWorking(in: botId) == true : threadTask?.status.isLive == true }

    private var isEmpty: Bool { draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && files.isEmpty }

    private var canSend: Bool {
        if let start { return start.ready && !isEmpty && starting == nil && model.connection == .online }
        // A thread's message has no mailbox to wait in.
        if thread != nil { return !isEmpty && !model.isOffline }
        // Offline computers still take text: the relay holds it for them, or with no relay this phone does.
        return !isEmpty && (model.connection == .online || ((model.canQueue || model.holdsOnPhone) && files.isEmpty))
    }

    /// A thread takes files only where its host says so (`talkFiles`).
    private var canAttach: Bool {
        guard start == nil, let bot else { return false }
        guard thread != nil else { return true }
        return threadTask?.talkFiles == true
    }

    private var placeholder: String {
        let name = bot?.name ?? ""
        if start != nil { return L10n.tr("What should it work on?") }
        if thread != nil { return L10n.tr("Message this thread") }
        if let artifact { return L10n.tr("Message about “{title}”").replacingOccurrences(of: "{title}", with: artifact.title) }
        if replyingTo != nil { return L10n.tr("Reply…") }
        if bot?.isGroup == true { return L10n.tr("Ask everyone · @ to ask one bot") }
        let key = working ? "Queue a message for {name}" : "Ask {name}"
        return L10n.tr(key).replacingOccurrences(of: "{name}", with: name)
    }

    /// The group's bots whose name holds the `@partial` at the end of the draft.
    private var suggestions: [Bot] {
        guard start == nil, bot?.isGroup == true, let query = mentionQuery(draft, members: members) else { return [] }
        return members.filter { query.isEmpty || $0.name.localizedCaseInsensitiveContains(query) }
    }

    /// A group's bots, which its commands act on.
    private var members: [Bot] { bot.map(model.members(of:)) ?? [] }

    /// The bot's `/` commands while the draft is `/partial`.
    private var commands: [SlashCommand] {
        guard thread == nil, start == nil, let query = SlashCommands.query(draft) else { return [] }
        return SlashCommands.list(for: bot, members: members, query: query, skills: model.installedSkills)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if !queued.isEmpty {
                QueuedMessages(entries: queued)
                    .padding(.horizontal, 16)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
            if !commands.isEmpty {
                CommandList(commands: commands, agentRegistry: model.agentRegistry, agentName: model.backendName, pick: run)
                    .padding(.horizontal, 16)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
            if !suggestions.isEmpty {
                MentionList(members: suggestions, pick: mention)
                    .padding(.horizontal, 16)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
            field
        }
        .animation(Motion.layout, value: suggestions.map(\.id))
        .animation(Motion.layout, value: commands.map(\.id))
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: queued.isEmpty)
    }

    private var buttonSize: CGFloat { 34 }

    private var field: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let replyingTo {
                quoteBar(replyingTo).transition(.move(edge: .bottom).combined(with: .opacity))
            }
            if !files.isEmpty {
                fileChips.transition(.move(edge: .bottom).combined(with: .opacity))
            }
            box
        }
        .glass(in: RoundedRectangle(cornerRadius: 24, style: .continuous))
        // Lines, not the text: an animated transaction on every keystroke breaks IME composition.
        .animation(Motion.layout, value: draft.count(where: \.isNewline))
        .animation(Motion.layout, value: files.map(\.id))
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: replyingTo?.id)
        .dropDestination(for: PickedFile.self) { picked, _ in
            guard canAttach else { return false }
            append(picked.map(\.file))
            return true
        }
        .fileImporter(isPresented: $pickingFiles, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
            if case .success(let urls) = result { add(urls) }
        }
            .quietPresentation(shown: pickingPhotos) {
                $0.photosPicker(isPresented: $pickingPhotos, selection: $photoItems, maxSelectionCount: 10, matching: .images)
            }
            .onChange(of: photoItems) { _, items in
                guard !items.isEmpty else { return }
                photoItems = []
                Task { await addPhotos(items) }
            }
        .padding(.horizontal, 16)
        .padding(.top, 6)
        .padding(.bottom, 16)
    }

    private enum Trailing: Equatable {
        case interrupt, stop, call, send
    }

    private var trailing: Trailing {
        if onInterrupt != nil, draft.isEmpty, files.isEmpty { return .interrupt }
        if working, draft.isEmpty, files.isEmpty { return .stop }
        if onCall != nil, draft.isEmpty, files.isEmpty { return .call }
        return .send
    }

    /// One button that morphs between call, send, stop and interrupt: its symbol replaces itself,
    /// its fill crossfades and the capsule narrows to a circle, so switching never pops.
    private var trailingButton: some View {
        let state = trailing
        let enabled = state != .send || canSend
        let (symbol, label, size): (String, String, CGFloat) = switch state {
        case .interrupt: ("stop.fill", "Interrupt", 12)
        case .stop: ("stop.fill", "Stop", 12)
        case .call: ("waveform", "Call", 15)
        case .send: ("arrow.up", "Send", 14)
        }
        let fill: Color = switch state {
        case .interrupt: Palette.danger
        case .send: canSend ? Palette.accentFill : Palette.accentDim
        case .stop, .call: Palette.accentFill
        }
        let ink: Color = switch state {
        case .interrupt: .white
        case .send: canSend ? Palette.onAccent : Palette.tertiary
        case .stop, .call: Palette.onAccent
        }
        return Button {
            switch state {
            case .interrupt: onInterrupt?()
            case .stop: if thread == nil { model.stop(botId) } else { stopThread?() }
            case .call:
                // The call takes the microphone.
                dictation.stop()
                onCall?()
            case .send: submit()
            }
        } label: {
            Image(systemName: symbol)
                .font(.system(size: size, weight: .bold))
                .contentTransition(.symbolEffect(.replace))
                .foregroundStyle(ink)
                .frame(width: state == .call ? buttonSize * 1.35 : buttonSize, height: buttonSize)
                .background(fill, in: Capsule())
                .contentShape(Capsule())
        }
        .buttonStyle(PressScale())
        .disabled(!enabled)
        .keyboardShortcut(state == .send ? KeyboardShortcut(.return, modifiers: .command) : nil)
        .accessibilityLabel(label)
        .help(label)
        .animation(Motion.reduced(Motion.morph, reduceMotion), value: state)
        .animation(Motion.reduced(Motion.morph, reduceMotion), value: canSend)
    }

    /// Dictation, beside send / call (Grok Bot's): a light circle with its engine menu; while it
    /// listens or transcribes, the recording pill.
    private var micButton: some View {
        Group {
            if dictation.busy {
                RecordingPill(dictation: dictation, height: buttonSize)
                    .transition(.opacity.combined(with: .scale(scale: 0.9, anchor: .trailing)))
            } else {
                HStack(spacing: 0) {
                    Button {
                        Task {
                            await dictation.start(after: draft, model: model) { change in draft = change(draft) }
                        }
                    } label: {
                        Image(systemName: "mic")
                            .font(.system(size: 15, weight: .medium))
                            .foregroundStyle(Palette.secondary)
                            .frame(width: buttonSize, height: buttonSize)
                            .contentShape(Circle())
                    }
                    .buttonStyle(PressScale())
                    .accessibilityLabel("Dictate")
                    .help("Dictate")
                    DictationEngineMenu(height: buttonSize)
                }
                // The mic and its engine menu share one light capsule.
                .background(Palette.bubbleAgent, in: Capsule())
                .transition(.opacity)
            }
        }
        .animation(Motion.reduced(Motion.morph, reduceMotion), value: dictation.busy)
        .codyncSheet(isPresented: $dictation.needsKey) { VoiceChatSettingsView() }
        .codyncDialog("Couldn't dictate", isPresented: Binding(get: { dictation.failure != nil }, set: { if !$0 { dictation.failure = nil } }),
                      message: dictation.failure, cancel: "OK") { [] }
    }

    /// Two layers, as on the desktop: the field on top; under it the + for files, what the bot runs on, the mic and
    /// send / stop / call.
    private var box: some View {
        VStack(alignment: .leading, spacing: 4) {
            TextField(placeholder, text: $input.text, selection: $input.selection, axis: .vertical)
                .lineLimit(1...8)
                .font(.body)
                .textFieldStyle(.plain)
                .focused($focused)
                // A hardware keyboard's Return (⌘↩ too) submits the field and ends editing.
                .onSubmit(newline)
                .padding(.horizontal, 10)
                .padding(.top, 8)
            HStack(spacing: 8) {
                if canAttach { addButton }
                Spacer(minLength: 0)
                // A bot's own chat: a group's bots each run on their own; a thread, on its task's agent, fixed.
                if let start { start.controls }
                if thread == nil, start == nil, let bot, !bot.isGroup { ModelChip(bot: bot) }
                if let bot, let threadTask { ThreadComposerChips(bot: bot, task: threadTask) }
                if thread == nil, start == nil { micButton.layoutPriority(1) }
                if starting != nil {
                    Spinner().frame(width: buttonSize, height: buttonSize)
                } else {
                    trailingButton
                        .layoutPriority(1)
                        .animation(Motion.layout, value: isEmpty)
                }
            }
        }
        .padding(8)
        .onChange(of: draftKey, initial: true) {
            dictation.stop()
            draft = model.composerDraft(for: draftKey)
        }
        // Another chat in the same view never inherits the reply.
        .onChange(of: botId) { replyingTo = nil }
        .onChange(of: replyingTo?.id) { if replyingTo != nil { focused = true } }
        .onDisappear {
            dictation.stop()
            starting?.cancel()
        }
        .onChange(of: draft) { _, value in
            model.setComposerDraft(value, for: draftKey)
            // Starting a command: list the computer's skills again.
            if value == "/" { Task { await model.refreshPlugins() } }
        }
    }

    /// The quoted message, one line, with a way to drop the reply.
    private func quoteBar(_ target: ReplyQuote) -> some View {
        HStack(spacing: 4) {
            QuoteLine(text: target.text)
            Spacer(minLength: 0)
            IconButton("Cancel reply", systemImage: "xmark") { replyingTo = nil }
        }
        .padding(.leading, 18)
        .padding(.trailing, 6)
        .padding(.top, 6)
    }

    /// Photos, files or the clipboard, from a system menu; dropping files on the box works too.
    @ViewBuilder private var addButton: some View {
            DropdownMenu {
                var items = [MenuItem("Photos", icon: "photo.on.rectangle") { pickingPhotos = true },
                             MenuItem("Files", icon: "folder") { pickingFiles = true }]
                if UIPasteboard.general.hasImages || UIPasteboard.general.hasURLs {
                    items.append(MenuItem("Paste", icon: "doc.on.clipboard") { pasteFiles() })
                }
                return items
            } label: { addLabel }
                .accessibilityLabel("Add files")
    }

    /// A light circle, as the mic's capsule.
    private var addLabel: some View {
        Image(systemName: "plus")
            .font(.system(size: 16, weight: .medium))
            .foregroundStyle(Palette.text)
            .frame(width: buttonSize, height: buttonSize)
            .background(Palette.bubbleAgent, in: Circle())
            .contentShape(Circle())
    }

    private var fileChips: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(files) { file in
                    ComposerAttachment(file: file) {
                        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) {
                            files.removeAll { $0.id == file.id }
                        }
                    }
                }
            }
            .padding(.horizontal, 10)
            .padding(.top, 8)
        }
    }

    private func add(_ urls: [URL]) {
        append(urls.compactMap { url in
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            return (try? Data(contentsOf: url, options: .mappedIfSafe)).map { OutgoingFile.prepared(name: url.lastPathComponent, data: $0) }
        })
    }

    private func append(_ new: [OutgoingFile]) {
        guard canAttach else { return }
        for file in new {
            guard file.data.count <= OutgoingFile.maxSize else {
                model.lastError = "\(file.name) is larger than 100 MB."
                continue
            }
            files.append(file)
        }
    }

    /// Images (or files) on the clipboard become attachments.
    private func pasteFiles() {
        let providers = UIPasteboard.general.itemProviders
        Task { append(await PickedFile.load(providers)) }
    }

        /// Photos go out as JPEG: HEIC isn't something every agent can read.
        private func addPhotos(_ items: [PhotosPickerItem]) async {
            let stamp = Int(Date.now.timeIntervalSince1970)
            var picked: [OutgoingFile] = []
            for (i, item) in items.enumerated() {
                guard let data = try? await item.loadTransferable(type: Data.self),
                      let jpeg = ImageData.jpeg(data) else { continue }
                picked.append(OutgoingFile(name: "photo-\(stamp)-\(i + 1).jpg", data: jpeg))
            }
            append(picked)
        }

    private func mention(_ member: Bot) {
        guard let at = draft.lastIndex(of: "@") else { return }
        draft = String(draft[..<at]) + "@\(member.name) "
        focused = true
    }

    private func run(_ command: SlashCommand) {
        guard let bot else { return }
        switch command.action {
        case .fast:
            model.toggleFastMode(bot)
            draft = ""
        case .skill(let id), .agent(let id, _):
            draft = "/\(id) "
        }
        focused = true
    }

    /// Return on a hardware keyboard: a new line at the cursor, and the field keeps the keyboard.
    /// An empty box gets none, also right after ⌘↩ sent it (the field submits after the shortcut).
    private func newline() {
        focused = true
        input.insertNewline()
    }

    private func submit() {
        // Sent once the words still on their way are in the box, with what was typed.
        if dictation.busy { return dictation.finish(then: submit) }
        // A start still waiting (key check, permission) never opens the microphone after the send.
        dictation.stop()
        if thread == nil, start == nil, files.isEmpty, let command = SlashCommands.builtIn(for: bot, members: members, draft: draft) { return run(command) }
        guard canSend else { return }
        if let start { return send(start) }
        // The store's copy follows the box a view update late: a transcript just put in is not in it yet.
        model.setComposerDraft(draft, for: draftKey)
        withAnimation(Motion.reduced(Motion.conversation, reduceMotion)) {
            guard model.sendComposerDraft(to: botId, draft: draftKey, replyTo: artifact?.replyQuote ?? replyingTo,
                                          files: files, thread: thread) else { return }
            draft = ""
            files = []
            replyingTo = nil
        }
    }

    /// A new thread's send: the box keeps its text until the computer took it.
    private func send(_ start: ComposerStart) {
        let text = draft
        starting = Task {
            if await start.send(text), !Task.isCancelled { draft = "" }
            starting = nil
        }
    }
}

/// A new thread's box (the desktop's composer with `thread.send`): its draft, whether it can go, what the thread runs
/// on in the box's row, and the send, true once the computer took it.
struct ComposerStart {
    var draftKey: String
    var ready: Bool
    var controls: AnyView
    var send: (String) async -> Bool
}

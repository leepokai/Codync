import CodyncKit
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

/// The message box under a chat: send, or stop while the bot (or the group) is working
/// there. A leading `/` lists commands (a group's act on its bots); in a group, typing `@`
/// suggests its bots.
struct Composer: View {
    let botId: String
    /// The message the next send quotes, shown above the field until sent or cancelled.
    @Binding var replyingTo: Entry?
    /// Starts a voice call; while the box is empty it takes the send button's place.
    var onCall: (() -> Void)?
    /// Interrupts the bot while it is speaking in a voice call.
    var onInterrupt: (() -> Void)?
    /// An artifact's viewer: every message is about the artifact (a reply to its card), from a draft of its own.
    var artifact: Artifact?
    @Environment(BotStore.self) private var model
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
    /// One line of the field's text, following Dynamic Type: the send button centers on the last line.
    @ScaledMetric(relativeTo: .body) private var lineHeight: CGFloat = 22

    private var bot: Bot? { model.bots[botId] }
    private var draftKey: String { artifact.map(BotStore.artifactDraftKey) ?? botId }
    private var working: Bool { bot?.isWorking(in: botId) == true }

    private var isEmpty: Bool { draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && files.isEmpty }

    private var canSend: Bool {
        // Offline computers still take messages when the relay can hold them for it (text only).
        !isEmpty && (model.connection == .online || (model.canQueue && files.isEmpty))
    }

    private var canAttach: Bool { bot != nil }

    private var placeholder: String {
        let name = bot?.name ?? ""
        if let artifact { return L10n.tr("Message about “{title}”").replacingOccurrences(of: "{title}", with: artifact.title) }
        if replyingTo != nil { return L10n.tr("Reply…") }
        if bot?.isGroup == true { return L10n.tr("Ask everyone · @ to ask one bot") }
        let key = working ? "Queue a message for {name}" : "Ask {name}"
        return L10n.tr(key).replacingOccurrences(of: "{name}", with: name)
    }

    /// The `@partial` being typed at the end, if any.
    private var mentionQuery: String? {
        guard bot?.isGroup == true, let at = draft.lastIndex(of: "@") else { return nil }
        let query = draft[draft.index(after: at)...]
        if at > draft.startIndex, !draft[draft.index(before: at)].isWhitespace { return nil }
        return query.contains(where: \.isNewline) || query.count > 24 ? nil : String(query)
    }

    private var suggestions: [Bot] {
        guard let query = mentionQuery else { return [] }
        return members.filter {
            query.isEmpty || $0.name.localizedCaseInsensitiveContains(query)
        }
    }

    /// A group's bots, which its commands act on.
    private var members: [Bot] { bot.map(model.members(of:)) ?? [] }

    /// The bot's `/` commands while the draft is `/partial`.
    private var commands: [SlashCommand] {
        guard let query = SlashCommands.query(draft) else { return [] }
        return SlashCommands.list(for: bot, members: members, query: query, skills: model.installedSkills)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if !commands.isEmpty {
                CommandList(commands: commands, pick: run)
                    .padding(.horizontal, 16)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
            if !suggestions.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 6) {
                        ForEach(suggestions) { member in
                            SuggestionChip(title: member.name) {
                                CharacterAvatar(bot: member, size: 18, animated: false)
                            } action: {
                                mention(member)
                            }
                        }
                    }
                    .padding(.horizontal, 16)
                }
                .transition(.move(edge: .bottom).combined(with: .opacity))
            }
            field
        }
        .animation(Motion.layout, value: suggestions.map(\.id))
        .animation(Motion.layout, value: commands.map(\.id))
    }

    private var fieldPadding: CGFloat { 10 }
    private var buttonSize: CGFloat { 34 }
    private var boxPadding: CGFloat { 6 }
    /// The one-line field's height; the + circle matches it.
    private var fieldHeight: CGFloat { max(textLineHeight + 2 * fieldPadding, buttonSize) + 2 * boxPadding }
    private var textLineHeight: CGFloat {
        lineHeight
    }

    private var field: some View {
        HStack(alignment: .bottom, spacing: 8) {
            if canAttach { addButton }
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
        }
        .animation(Motion.layout, value: files.map(\.id))
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: replyingTo?.id)
        .dropDestination(for: PickedFile.self) { picked, _ in
            guard canAttach else { return false }
            append(picked.map(\.file))
            return true
        }
        .reviewInterruption(active: focused || pickingFiles || pickingPhotos || !files.isEmpty
                            || dictation.listening || dictation.transcribing)
        .fileImporter(isPresented: $pickingFiles, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
            if case .success(let urls) = result { add(urls) }
        }
            .photosPicker(isPresented: $pickingPhotos, selection: $photoItems, maxSelectionCount: 10, matching: .images)
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
            case .stop: model.stop(botId)
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
            if dictation.listening || dictation.transcribing {
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
        .animation(Motion.reduced(Motion.morph, reduceMotion), value: dictation.listening || dictation.transcribing)
        .codyncSheet(isPresented: $dictation.needsKey) { VoiceChatSettingsView() }
        .codyncDialog("Couldn't dictate", isPresented: Binding(get: { dictation.failure != nil }, set: { if !$0 { dictation.failure = nil } }),
                      message: dictation.failure, cancel: "OK") { [] }
    }

    private var box: some View {
        HStack(alignment: .bottom, spacing: 8) {
            TextField(placeholder, text: $input.text, selection: $input.selection, axis: .vertical)
                .lineLimit(1...8)
                .font(.body)
                .textFieldStyle(.plain)
                .focused($focused)
                // A hardware keyboard's Return (⌘↩ too) submits the field and ends editing.
                .onSubmit(newline)
                .padding(.vertical, fieldPadding)
            micButton
                .padding(.bottom, max(0, (textLineHeight + 2 * fieldPadding - buttonSize) / 2))
            trailingButton
                .padding(.bottom, max(0, (textLineHeight + 2 * fieldPadding - buttonSize) / 2))
                .animation(Motion.layout, value: isEmpty)
        }
        .padding(.leading, 18)
        .padding(.trailing, 6)
        .padding(.vertical, boxPadding)
        .onChange(of: draftKey, initial: true) {
            dictation.stop()
            draft = model.composerDraft(for: draftKey)
        }
        // Another chat in the same view never inherits the reply.
        .onChange(of: botId) { replyingTo = nil }
        .onChange(of: replyingTo?.id) { if replyingTo != nil { focused = true } }
        .onDisappear { dictation.stop() }
        .onChange(of: draft) { _, value in
            model.setComposerDraft(value, for: draftKey)
            // Starting a command: list the computer's skills again.
            if value == "/" { Task { await model.refreshPlugins() } }
        }
    }

    /// The quoted message, one line, with a way to drop the reply.
    private func quoteBar(_ target: Entry) -> some View {
        HStack(spacing: 4) {
            QuoteLine(text: target.replyQuote.text)
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

    private var addLabel: some View {
        Image(systemName: "plus")
            .font(.system(size: 18, weight: .medium))
            .foregroundStyle(Palette.text)
            .frame(width: fieldHeight, height: fieldHeight)
            .glass(in: Circle())
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
        case .skill(let id):
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
        // Words still on their way would land in the emptied box.
        dictation.stop()
        if files.isEmpty, let command = SlashCommands.builtIn(for: bot, members: members, draft: draft) { return run(command) }
        guard canSend else { return }
        withAnimation(Motion.reduced(Motion.conversation, reduceMotion)) {
            guard model.sendComposerDraft(to: botId, draft: draftKey, replyTo: artifact?.replyQuote ?? replyingTo?.replyQuote,
                                          files: files) else { return }
            draft = ""
            files = []
            replyingTo = nil
        }
    }
}

/// One suggestion above the box: a bot to @mention.
private struct SuggestionChip<Icon: View>: View {
    let title: String
    @ViewBuilder let icon: Icon
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                icon
                Text(title).font(.subheadline).foregroundStyle(Palette.text)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(Palette.surface, in: Capsule())
        }
        .buttonStyle(PressScale())
    }
}

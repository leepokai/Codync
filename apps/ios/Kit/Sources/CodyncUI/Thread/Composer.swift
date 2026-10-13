import CodyncKit
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

/// The message box under a chat or a thread: send, or stop while the bot (or the
/// group) is working there. In a group, typing `@` suggests its bots; with one bot, a
/// leading `/` suggests its skills.
struct Composer: View {
    let botId: String
    /// Replies go to the thread on this message.
    var thread: String?
    /// Starts a voice call; while the box is empty it takes the send button's place.
    var onCall: (() -> Void)?
    /// Interrupts the bot while it is speaking in a voice call.
    var onInterrupt: (() -> Void)?
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// The field owns its text; the store only keeps a copy. A field bound straight to the
    /// store loses a hardware keyboard's IME composition (Zhuyin, Pinyin, Kana) on iPad.
    @State private var draft = ""
    /// Files going out with the next message.
    @State private var files: [OutgoingFile] = []
    @State private var pickingFiles = false
    @State private var pickingPhotos = false
    @State private var photoItems: [PhotosPickerItem] = []
    @FocusState private var focused: Bool
    /// One line of the field's text, following Dynamic Type: the send button centers on the last line.
    @ScaledMetric(relativeTo: .body) private var lineHeight: CGFloat = 22

    private var bot: Bot? { model.bots[botId] }
    private var working: Bool { bot?.isWorking(in: botId, thread: thread) == true }

    private var isEmpty: Bool { draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && files.isEmpty }

    private var canSend: Bool {
        // Offline computers still take messages when the relay can hold them for it (text only).
        !isEmpty && (model.connection == .online || (model.canQueue && files.isEmpty))
    }

    private var canAttach: Bool { bot != nil }

    private var placeholder: String {
        let name = bot?.name ?? ""
        if thread != nil { return "Reply…" }
        if bot?.isGroup == true { return "Message everyone · @ to ask one bot" }
        return working ? "Queue a message for \(name)" : "Ask \(name)"
    }

    /// The `@partial` being typed at the end, if any.
    private var mentionQuery: String? {
        guard bot?.isGroup == true, let at = draft.lastIndex(of: "@") else { return nil }
        let query = draft[draft.index(after: at)...]
        if at > draft.startIndex, !draft[draft.index(before: at)].isWhitespace { return nil }
        return query.contains(where: \.isNewline) || query.count > 24 ? nil : String(query)
    }

    private var suggestions: [Bot] {
        guard let query = mentionQuery, let bot else { return [] }
        return model.members(of: bot).filter {
            query.isEmpty || $0.name.localizedCaseInsensitiveContains(query)
        }
    }

    /// The bot's skills while the draft is `/partial`; not in a group, where each member has its own.
    private var skillSuggestions: [InstalledSkill] {
        guard let bot, !bot.isGroup, !AppVersion.isBelow(model.hello?.version ?? "", SkillCommand.minHost),
              let query = SkillCommand.query(draft) else { return [] }
        return SkillCommand.matches(model.installedSkills, enabled: bot.skills, query: query)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if !suggestions.isEmpty || !skillSuggestions.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 6) {
                        ForEach(suggestions) { member in
                            SuggestionChip(title: member.name) {
                                CharacterAvatar(bot: member, size: 18, animated: false)
                            } action: {
                                mention(member)
                            }
                        }
                        ForEach(skillSuggestions) { skill in
                            SuggestionChip(title: skill.name) {
                                Image(systemName: "book.pages").font(.caption).foregroundStyle(Palette.secondary)
                            } action: {
                                draft = "/\(skill.id) "
                                focused = true
                            }
                            .help(skill.description)
                            .accessibilityHint(skill.description)
                        }
                    }
                    .padding(.horizontal, 16)
                }
                .transition(.move(edge: .bottom).combined(with: .opacity))
            }
            field
        }
        .animation(Motion.layout, value: suggestions.map(\.id))
        .animation(Motion.layout, value: skillSuggestions.map(\.id))
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
        .dropDestination(for: PickedFile.self) { picked, _ in
            guard canAttach else { return false }
            append(picked.map(\.file))
            return true
        }
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
            case .call: onCall?()
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
        .accessibilityLabel(label)
        .help(label)
        .animation(Motion.reduced(Motion.morph, reduceMotion), value: state)
        .animation(Motion.reduced(Motion.morph, reduceMotion), value: canSend)
    }

    private var box: some View {
        HStack(alignment: .bottom, spacing: 8) {
            TextField(placeholder, text: $draft, axis: .vertical)
                .lineLimit(1...8)
                .font(.body)
                .textFieldStyle(.plain)
                .focused($focused)
                .padding(.vertical, fieldPadding)
            trailingButton
                .padding(.bottom, max(0, (textLineHeight + 2 * fieldPadding - buttonSize) / 2))
                .animation(Motion.layout, value: isEmpty)
        }
        .padding(.leading, 18)
        .padding(.trailing, 6)
        .padding(.vertical, boxPadding)
        .onChange(of: [botId, thread], initial: true) {
            draft = model.composerDraft(for: botId, thread: thread)
        }
        .onChange(of: draft) { _, value in
            model.setComposerDraft(value, for: botId, thread: thread)
            // Starting a skill name: list the computer's skills again.
            if value == "/", bot?.isGroup == false { Task { await model.refreshPlugins() } }
        }
        .onChange(of: model.routineDrafts[botId]) { _, value in
            guard thread == nil, let value else { return }
            draft = draft.isEmpty ? value : draft + "\n" + value
            model.routineDrafts.removeValue(forKey: botId)
            focused = true
        }
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

    private func submit() {
        guard canSend else { return }
        withAnimation(Motion.reduced(Motion.conversation, reduceMotion)) {
            guard model.sendComposerDraft(to: botId, thread: thread, files: files) else { return }
            draft = ""
            files = []
        }
    }
}

/// One suggestion above the box: a bot to @mention or a skill to start with.
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

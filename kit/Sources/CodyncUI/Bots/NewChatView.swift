import CodyncKit
import SwiftUI

/// Desktop "new message" page (Grok Bot's compose): a To: field that searches your
/// bots or creates a new one, with ⌘1…⌘9 shortcuts, and the message box underneath.
/// Picked bots become chips in the To: field; one opens its chat, several start a
/// group chat with them (or open the one they already share). Whatever you typed is sent.
public struct NewChatView: View {
    let close: () -> Void
    @Environment(BotStore.self) private var model
    @State private var query = ""
    @State private var draft = ""
    @State private var recipients: [String] = []
    @State private var creating = false
    @FocusState private var toFocused: Bool

    public init(close: @escaping () -> Void) { self.close = close }

    /// Bots not picked yet; a group can only be opened on its own, so groups go once someone's picked.
    private var matches: [Bot] {
        let q = query.trimmingCharacters(in: .whitespaces)
        return model.roster.filter {
            !recipients.contains($0.id) && !($0.isGroup && !recipients.isEmpty)
                && (q.isEmpty || $0.name.localizedCaseInsensitiveContains(q))
        }
    }

    private var picked: [Bot] { recipients.compactMap { model.bots[$0] } }

    public var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Text("To:").foregroundStyle(Palette.secondary)
                ScrollViewReader { proxy in
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 6) {
                            ForEach(picked) { bot in
                                BotChip(bot: bot) { remove(bot.id) }
                                    .transition(.scale(scale: 0.85).combined(with: .opacity))
                            }
                            TextField(recipients.isEmpty ? "Search or create bots" : "Add another bot", text: $query)
                                .textFieldStyle(.plain)
                                .focused($toFocused)
                                .frame(minWidth: 200)
                                .onSubmit(submitTo)
                                .onKeyPress(.delete) {
                                    // Backspace in an empty field takes the last chip back out.
                                    guard query.isEmpty, let last = recipients.last else { return .ignored }
                                    remove(last)
                                    return .handled
                                }
                                .id("field")
                        }
                        .animation(Motion.layout, value: recipients)
                    }
                    .onChange(of: recipients) { _, _ in withAnimation(Motion.layout) { proxy.scrollTo("field", anchor: .trailing) } }
                }
                IconButton("Close", systemImage: "xmark", action: close)
                    .keyboardShortcut(.cancelAction)
            }
            .font(.title3)
            .padding(.horizontal, 22)
            .padding(.vertical, 12)
            Rectangle().fill(Palette.border).frame(height: 0.5)

            ScrollView {
                VStack(spacing: 2) {
                    PickRow(shortcut: 1, action: create) {
                        Image(systemName: creating ? "hourglass" : "plus")
                            .font(.system(size: 13, weight: .medium))
                            .frame(width: 26, height: 26)
                            .background(Palette.bubbleAgent, in: Circle())
                    } label: {
                        Text(query.isEmpty ? "Create new Bot" : "Create “\(query)”")
                    }
                    ForEach(Array(matches.enumerated()), id: \.element.id) { i, bot in
                        PickRow(shortcut: i + 2 <= 9 ? i + 2 : nil, action: { choose(bot) }) {
                            if bot.isGroup {
                                GroupAvatar(members: model.members(of: bot), size: 26, animated: false)
                            } else {
                                CharacterAvatar(bot: bot, size: 26, animated: false)
                            }
                        } label: {
                            Text(bot.name)
                        }
                    }
                }
                .padding(8)
            }
            .frame(maxHeight: 460)
            .fixedSize(horizontal: false, vertical: true)
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
            .shadow(color: .black.opacity(0.08), radius: 16, y: 6)
            .frame(maxWidth: 620, alignment: .leading)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 22)
            .padding(.top, 10)
            .animation(Motion.layout, value: matches.map(\.id))

            Spacer(minLength: 0)

            HStack(alignment: .bottom, spacing: 8) {
                TextField(placeholder, text: $draft, axis: .vertical)
                    .lineLimit(1...6)
                    .textFieldStyle(.plain)
                    .padding(.vertical, 11)
                    .sendOnReturn(start)
            }
            .padding(.leading, 18)
            .padding(.trailing, 6)
            .padding(.vertical, 6)
            .composerSurface(in: RoundedRectangle(cornerRadius: 24, style: .continuous))
            .padding(16)
        }
        .background(Palette.background)
        .task {
            await Task.yield()
            toFocused = true
        }
    }

    private var placeholder: String {
        switch picked.count {
        case 0: "Message Bot"
        case 1: "Message \(picked[0].name)"
        default: "Message \(picked.map(\.name).joined(separator: ", "))"
        }
    }

    /// A bot becomes a chip; a group (with nobody picked) opens right away.
    private func choose(_ bot: Bot) {
        if bot.isGroup {
            open(bot.id)
            return
        }
        withAnimation(Motion.layout) { recipients.append(bot.id) }
        query = ""
        toFocused = true
    }

    private func remove(_ id: String) {
        withAnimation(Motion.layout) { recipients.removeAll { $0 == id } }
    }

    /// Return in To: picks the top match, or with the field empty, starts the chat.
    private func submitTo() {
        if !query.trimmingCharacters(in: .whitespaces).isEmpty {
            if let first = matches.first { choose(first) } else { create() }
        } else {
            start()
        }
    }

    /// One bot: its chat. Several: their group chat.
    private func start() {
        switch recipients.count {
        case 0: return
        case 1: open(recipients[0])
        default:
            let names = picked.map(\.name).joined(separator: ", ")
            Task {
                do {
                    let group = try await model.createGroup(name: names, members: recipients)
                    open(group.id)
                } catch {
                    model.lastError = error.localizedDescription
                }
            }
        }
    }

    private func open(_ id: String) {
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        model.selection = id
        if !text.isEmpty { model.send(text, to: id) }
        close()
    }

    private func create() {
        guard !creating else { return }
        creating = true
        Task {
            do {
                var bot = try await model.createDefaultBot()
                let name = query.trimmingCharacters(in: .whitespaces)
                if !name.isEmpty {
                    var d = BotDraft(bot)
                    d.name = name
                    bot = try await model.save(d)
                }
                if recipients.isEmpty {
                    open(bot.id)
                } else {
                    // `createDefaultBot` selected it; stay here and add it to the others.
                    model.selection = nil
                    choose(bot)
                }
            } catch {
                model.lastError = error.localizedDescription
            }
            creating = false
        }
    }
}

private struct PickRow<Icon: View, Label: View>: View {
    let shortcut: Int?
    let action: () -> Void
    @ViewBuilder let icon: Icon
    @ViewBuilder let label: Label
    @State private var hovering = false

    var body: some View {
        if let shortcut {
            row.keyboardShortcut(KeyEquivalent(Character("\(shortcut)")), modifiers: .command)
        } else {
            row
        }
    }

    private var row: some View {
        Button(action: action) {
            HStack(spacing: 12) {
                icon
                label.foregroundStyle(Palette.text).lineLimit(1)
                Spacer()
                if let shortcut {
                    HStack(spacing: 3) {
                        KeyCap("⌘")
                        KeyCap("\(shortcut)")
                    }
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(hovering ? Palette.bubbleAgent : .clear, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            .contentShape(Rectangle())
        }
        .buttonStyle(PressScale())
        .onHover { h in withAnimation(Motion.hover) { hovering = h } }
    }
}

private struct KeyCap: View {
    let text: String
    init(_ text: String) { self.text = text }

    var body: some View {
        Text(text)
            .font(.caption.monospaced())
            .foregroundStyle(Palette.secondary)
            .frame(minWidth: 18, minHeight: 18)
            .background(Palette.background, in: RoundedRectangle(cornerRadius: 4))
    }
}

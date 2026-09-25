import CodyncKit
import SwiftUI

/// Desktop "new message" page (Grok Bot's compose): a To: field that searches
/// your bots or creates a new one, with ⌘1…⌘9 shortcuts, and the message box
/// underneath. Whatever you typed is sent to the bot you pick.
public struct NewChatView: View {
    let close: () -> Void
    @Environment(BotStore.self) private var model
    @State private var query = ""
    @State private var draft = ""
    @State private var creating = false
    @FocusState private var toFocused: Bool

    public init(close: @escaping () -> Void) { self.close = close }

    private var matches: [Bot] {
        let q = query.trimmingCharacters(in: .whitespaces)
        return model.roster.filter { q.isEmpty || $0.name.localizedCaseInsensitiveContains(q) }
    }

    public var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Text("To:").foregroundStyle(Palette.secondary)
                TextField("Search or create bots", text: $query)
                    .textFieldStyle(.plain)
                    .focused($toFocused)
                    .onSubmit { if let first = matches.first { pick(first) } else { create() } }
                Button("Close", systemImage: "xmark", action: close)
                    .labelStyle(.iconOnly)
                    .buttonStyle(.plain)
                    .foregroundStyle(Palette.secondary)
                    .keyboardShortcut(.cancelAction)
            }
            .font(.title3)
            .padding(.horizontal, 22)
            .padding(.vertical, 16)
            Divider().overlay(Palette.border)

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
                        PickRow(shortcut: i + 2 <= 9 ? i + 2 : nil, action: { pick(bot) }) {
                            CharacterAvatar(bot: bot, size: 26, animated: false)
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
            .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).stroke(Palette.border))
            .shadow(color: .black.opacity(0.08), radius: 16, y: 6)
            .frame(maxWidth: 620, alignment: .leading)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 22)
            .padding(.top, 10)

            Spacer(minLength: 0)

            HStack(alignment: .bottom, spacing: 8) {
                TextField("Message Bot", text: $draft, axis: .vertical)
                    .lineLimit(1...6)
                    .textFieldStyle(.plain)
                    .padding(.vertical, 11)
            }
            .padding(.leading, 18)
            .padding(.trailing, 6)
            .padding(.vertical, 6)
            .composerSurface(in: RoundedRectangle(cornerRadius: 24, style: .continuous))
            .padding(16)
        }
        .background(Palette.background)
        .onAppear { toFocused = true }
    }

    private func pick(_ bot: Bot) {
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        model.selection = bot.id
        if !text.isEmpty { model.send(text, to: bot.id) }
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
                pick(bot)
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
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
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
            .overlay(RoundedRectangle(cornerRadius: 4).stroke(Palette.border))
    }
}

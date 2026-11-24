import CodyncKit
import SwiftUI

/// The bot's chat starts over: what the new conversation carries, which the user can edit, or turn down to carry on.
struct HandoffCardRow: View {
    let entry: Entry
    let card: HandoffCard
    @Environment(BotStore.self) private var model
    @State private var expanded = false
    @State private var editing = false

    var body: some View {
        if card.isDraft {
            VStack(alignment: .leading, spacing: 10) {
                Label(entry.data.text ?? "", systemImage: "arrow.triangle.branch")
                    .font(.footnote)
                    .foregroundStyle(Palette.secondary)
                Disclosure(isExpanded: $expanded) {
                    carried
                } label: {
                    Text(L10n.tr("What it carries")).font(.footnote.weight(.medium)).foregroundStyle(Palette.text)
                }
                HStack(spacing: 8) {
                    if model.answering[entry.id] != nil {
                        Spinner()
                    } else {
                        Button(L10n.tr("Keep this conversation")) { model.keepConversation(entry) }
                            .buttonStyle(SecondaryButtonStyle())
                        IconButton("Edit what it carries", systemImage: "pencil") {
                            withAnimation(Motion.layout) { editing = true }
                        }
                    }
                }
            }
            .padding(12)
            .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 14))
            .padding(.trailing, 40)
            .codyncSheet(isPresented: $editing) { HandoffEditorView(entry: entry, text: card.text ?? "") }
        } else {
            Disclosure(isExpanded: $expanded) {
                carried
            } label: {
                Label(entry.data.text ?? "", systemImage: "arrow.triangle.branch")
                    .font(.footnote)
                    .foregroundStyle(Palette.secondary)
            }
        }
    }

    private var carried: some View {
        Text(card.text ?? "")
            .font(.footnote.monospaced())
            .foregroundStyle(Palette.secondary)
            .textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// The bot moved to another agent of its chain, or back to its own: one line, like a fresh conversation's, that
/// unfolds to what the new agent was told.
struct AgentSwitchRow: View {
    let entry: Entry
    let change: AgentSwitch
    @State private var expanded = false

    var body: some View {
        if change.manual == true {
            UserSwitchRow(change: change)
        } else if let carried = change.carried, !carried.isEmpty {
            Disclosure(isExpanded: $expanded) {
                CarriedText(text: carried)
            } label: {
                line
            }
        } else {
            line.frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var line: some View {
        Label(entry.data.text ?? "", systemImage: "arrow.turn.down.right")
            .font(.footnote)
            .foregroundStyle(Palette.secondary)
    }
}

/// The user moved the bot to another login: a divider saying its next message carries the conversation, and once it
/// went, one that unfolds to what was carried.
struct UserSwitchRow: View {
    let change: AgentSwitch
    @Environment(BotStore.self) private var model
    @State private var expanded = false

    var body: some View {
        VStack(spacing: 8) {
            HStack(spacing: 10) {
                rule
                if let carried = change.carried, !carried.isEmpty {
                    Button { withAnimation(Motion.layout) { expanded.toggle() } } label: { line }
                        .buttonStyle(.plain)
                } else {
                    line
                }
                rule
            }
            if expanded, let carried = change.carried {
                CarriedText(text: carried)
            }
        }
    }

    private var state: String? {
        switch change.state {
        case .pending: "Context handoff pending"
        case .handedOff: "Context handed off"
        case .switched: "Switched"
        case nil: nil
        }
    }

    private var line: some View {
        HStack(spacing: 6) {
            Image(systemName: "arrow.left.arrow.right")
            if let state { Text(L10n.tr(state)) }
            side(change.from, change.fromName)
            Image(systemName: "arrow.right").imageScale(.small)
            side(change.to, change.toName)
        }
        .font(.caption)
        .foregroundStyle(Palette.secondary)
        .lineLimit(1)
        .minimumScaleFactor(0.8)
    }

    private func side(_ backend: String?, _ name: String?) -> some View {
        HStack(spacing: 4) {
            AgentIcon(registry: backend.map(model.agentRegistry), size: 12)
            Text(name ?? "").foregroundStyle(Palette.text)
        }
    }

    private var rule: some View {
        Rectangle().fill(Palette.border).frame(height: 1).frame(maxWidth: .infinity)
    }
}

/// What an agent taking over was told, unfolded under its line.
private struct CarriedText: View {
    let text: String

    var body: some View {
        Text(text)
            .font(.footnote)
            .foregroundStyle(Palette.secondary)
            .textSelection(.enabled)
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 12))
            .transition(.opacity)
    }
}

/// Edits what the next conversation starts with.
struct HandoffEditorView: View {
    let entry: Entry
    @State private var text: String
    @State private var saving = false
    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss

    init(entry: Entry, text: String) {
        self.entry = entry
        _text = State(initialValue: text)
    }

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader("Edit the handoff") {
                if saving {
                    Spinner()
                } else {
                    IconButton("Save", systemImage: "checkmark") {
                        saving = true
                        Task {
                            if await model.editHandoff(entry, text: trimmed) { dismiss() }
                            saving = false
                        }
                    }
                    .disabled(trimmed.isEmpty)
                }
            }
            CardForm {
                CardSection(footer: "Your next message starts a fresh conversation that begins with this.") {
                    TextField("What the new conversation should know", text: $text, axis: .vertical)
                        .lineLimit(10...24)
                        .font(.footnote.monospaced())
                }
            }
            .textFieldStyle(.plain)
        }
        .background(Palette.background)
    }

    private var trimmed: String { text.trimmingCharacters(in: .whitespacesAndNewlines) }
}

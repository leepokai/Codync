import CodyncKit
import SwiftUI

/// The host owns selection and expiry; a tap only requests an answer and waits for its streamed state.
/// The last row sends the user's own words as an ordinary message, which the host takes as the answer.
struct DecisionCard: View {
    let entry: Entry
    var answering: String?
    let respond: (String) -> Void
    let reply: (String) -> Void
    @State private var expanded = false
    @State private var shownHeight: CGFloat = 0
    @State private var fullHeight: CGFloat = 0

    private var d: EntryData { entry.data }
    private var pending: Bool { d.decisionStatus == .pending }
    /// The clamped question hides lines: only then is there something to expand.
    private var clamped: Bool { fullHeight > shownHeight + 1 }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .top, spacing: 8) {
                Text(d.question ?? "")
                    .font(.headline).foregroundStyle(Palette.text)
                    .lineLimit(expanded ? nil : 4).textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { shownHeight = $0 }
                    .background(alignment: .top) {
                        Text(d.question ?? "").font(.headline).fixedSize(horizontal: false, vertical: true).hidden()
                            .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { fullHeight = $0 }
                    }
                if expanded || clamped {
                    IconButton(expanded ? "Collapse" : "Expand", systemImage: expanded ? "chevron.up" : "chevron.down") {
                        Motion.animate { expanded.toggle() }
                    }
                    .accessibilityIdentifier("decision-expand")
                }
            }
            if pending {
                VStack(spacing: 2) {
                    ForEach(Array((d.choices ?? []).enumerated()), id: \.offset) { index, choice in
                        option(choice, index: index)
                    }
                    OwnAnswer(reply: reply)
                        .disabled(answering != nil).opacity(answering == nil ? 1 : 0.4)
                }
                .padding(.horizontal, -8)
            } else {
                Text(d.decisionStatus == .answered ? (d.selected ?? L10n.tr("Answered")) : L10n.tr("No answer — the bot moved on"))
                    .font(.caption.weight(.medium)).foregroundStyle(Palette.secondary)
                    .accessibilityIdentifier("decision-outcome")
            }
        }
        .padding(16).frame(maxWidth: 420, alignment: .leading)
        .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
        .padding(.trailing, 40)
        .accessibilityElement(children: .contain).accessibilityIdentifier("decision-card")
    }

    private func option(_ choice: String, index: Int) -> some View {
        Button { respond(choice) } label: {
            HStack(spacing: 10) {
                RowNumber {
                    if answering == choice { Spinner(size: 12) } else { Text(verbatim: "\(index + 1)") }
                }
                Text(choice).font(.body.weight(d.recommended == index ? .semibold : .regular))
                    .foregroundStyle(Palette.text).multilineTextAlignment(.leading)
                Spacer(minLength: 0)
                if d.recommended == index {
                    Text(L10n.tr("Suggested")).font(.caption).foregroundStyle(Palette.tertiary)
                }
            }
            .decisionRow()
        }
        .buttonStyle(RowStyle()).disabled(answering != nil)
        .opacity(answering == nil || answering == choice ? 1 : 0.4)
        .accessibilityIdentifier("decision-option-\(index)")
    }
}

/// The last row: the user's own answer, sent with Return or the send button. A pencil marks it, not a number:
/// a typed number only ever picks an option.
private struct OwnAnswer: View {
    let reply: (String) -> Void
    @State private var text = ""
    @FocusState private var typing: Bool

    private var trimmed: String { text.trimmingCharacters(in: .whitespacesAndNewlines) }

    var body: some View {
        HStack(spacing: 10) {
            RowNumber { Image(systemName: "pencil") }
            TextField(L10n.tr("Something else"), text: $text,
                      prompt: Text(L10n.tr("Something else")).foregroundStyle(Palette.tertiary))
                .font(.body).foregroundStyle(Palette.text)
                .focused($typing).submitLabel(.send).onSubmit(send)
                .accessibilityIdentifier("decision-own")
            if !trimmed.isEmpty {
                IconButton("Send", systemImage: "arrow.up", action: send)
                    .accessibilityIdentifier("decision-own-send")
            }
        }
        .decisionRow()
        .background(Palette.text.opacity(typing ? 0.06 : 0), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .animation(Motion.hover, value: typing)
        .onTapGesture { typing = true }
    }

    private func send() {
        guard !trimmed.isEmpty else { return }
        reply(trimmed)
        text = ""
        typing = false
    }
}

/// A row's number, or the own answer's pencil. VoiceOver reads the row itself.
private struct RowNumber<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        content
            .font(.caption.weight(.semibold).monospacedDigit()).foregroundStyle(Palette.secondary)
            .frame(width: 24, height: 24)
            .background(Palette.text.opacity(0.06), in: RoundedRectangle(cornerRadius: 7, style: .continuous))
            .accessibilityHidden(true)
    }
}

/// An option row lights up while pressed.
private struct RowStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .background(Palette.text.opacity(configuration.isPressed ? 0.08 : 0),
                        in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            .animation(Motion.press, value: configuration.isPressed)
    }
}

private extension View {
    func decisionRow() -> some View {
        frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
            .padding(.horizontal, 8).padding(.vertical, 4).contentShape(Rectangle())
    }
}

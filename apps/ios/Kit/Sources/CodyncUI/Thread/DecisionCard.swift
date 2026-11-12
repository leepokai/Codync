import CodyncKit
import SwiftUI

/// The host owns selection and expiry; a tap only requests an answer and waits for its streamed state.
struct DecisionCard: View {
    let entry: Entry
    var answering: String?
    let respond: (String) -> Void
    @State private var expanded = false
    @State private var shownHeight: CGFloat = 0
    @State private var fullHeight: CGFloat = 0

    private var d: EntryData { entry.data }
    private var pending: Bool { d.decisionStatus == .pending }
    /// The clamped question hides lines: only then is there something to expand.
    private var clamped: Bool { fullHeight > shownHeight + 1 }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                Text(L10n.tr("Needs your choice")).font(.headline).foregroundStyle(Palette.text)
                if pending { Circle().fill(Palette.warning).frame(width: 7, height: 7) }
            }
            Text(d.question ?? "")
                .font(.subheadline).foregroundStyle(Palette.text)
                .lineLimit(expanded ? nil : 4).textSelection(.enabled)
                .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { shownHeight = $0 }
                .background(alignment: .top) {
                    Text(d.question ?? "").font(.subheadline).fixedSize(horizontal: false, vertical: true).hidden()
                        .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { fullHeight = $0 }
                }
            if pending {
                VStack(spacing: 0) {
                    ForEach(Array((d.choices ?? []).enumerated()), id: \.offset) { index, choice in
                        if index > 0 { Rectangle().fill(Palette.border).frame(height: 0.5) }
                        Button { respond(choice) } label: {
                            HStack(spacing: 8) {
                                Text(choice).font(.body.weight(d.recommended == index ? .semibold : .regular))
                                    .foregroundStyle(Palette.text).multilineTextAlignment(.leading)
                                Spacer(minLength: 0)
                                if d.recommended == index {
                                    Text(L10n.tr("Suggested")).font(.caption).foregroundStyle(Palette.tertiary)
                                }
                                if answering == choice { Spinner(size: 14) }
                            }
                            .frame(maxWidth: .infinity, minHeight: 46, alignment: .leading)
                            .padding(.horizontal, 14).padding(.vertical, 6).contentShape(Rectangle())
                        }
                        .buttonStyle(.plain).disabled(answering != nil)
                        .opacity(answering == nil || answering == choice ? 1 : 0.4)
                        .accessibilityIdentifier("decision-option-\(index)")
                    }
                }
                .background(Palette.background, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            } else {
                Text(d.decisionStatus == .answered ? (d.selected ?? L10n.tr("Answered")) : L10n.tr("No answer — the bot moved on"))
                    .font(.caption.weight(.medium)).foregroundStyle(Palette.secondary)
                    .accessibilityIdentifier("decision-outcome")
            }
            if expanded || clamped {
                IconButton(expanded ? "Collapse" : "Expand", systemImage: expanded ? "chevron.up" : "chevron.down") {
                    Motion.animate { expanded.toggle() }
                }
                .accessibilityIdentifier("decision-expand")
            }
        }
        .padding(16).frame(maxWidth: 420, alignment: .leading)
        .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
        .padding(.trailing, 40)
        .accessibilityElement(children: .contain).accessibilityIdentifier("decision-card")
    }
}

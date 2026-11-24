import SwiftUI

// Codync's page and card layout: shared by the app (CodyncUI) and the share extension.

// MARK: - Forms

/// A scrolling page of cards: the replacement for `Form` / grouped `List`.
public struct CardForm<Content: View>: View {
    let content: Content

    public init(@ViewBuilder content: () -> Content) { self.content = content() }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 32) {
                content
            }
            .font(.body)
            .padding(20)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .scrollDismissesKeyboard(.interactively)
        .background(Palette.background)
    }
}

/// A titled group of rows: the replacement for `Section`. Modeled on ChatGPT's desktop settings:
/// a small bold heading over a filled, rounded group whose rows are split by inset hairlines
/// (references in `docs/design/reference/`).
public struct CardSection<Content: View, Accessory: View>: View {
    let title: String?
    let footer: String?
    /// The key a section that folds is kept folded by, on this device; nil for one that doesn't fold.
    let fold: String?
    let content: Content
    /// Trailing controls on the heading's line (add, refresh, clear).
    let accessory: Accessory
    @Environment(\.cardStyle) private var cardStyle
    @AppStorage("foldedCardSections") private var foldedKeys = ""

    public init(_ title: String? = nil, footer: String? = nil, fold: String? = nil, @ViewBuilder content: () -> Content,
                @ViewBuilder accessory: () -> Accessory) {
        self.title = title
        self.footer = footer
        self.fold = fold
        self.content = content()
        self.accessory = accessory()
    }

    private var folded: Bool { fold.map { foldedKeys.split(separator: ",").contains(Substring($0)) } ?? false }

    private func toggle() {
        guard let fold else { return }
        let keys = foldedKeys.split(separator: ",").map(String.init)
        withAnimation(Motion.layout) {
            foldedKeys = (folded ? keys.filter { $0 != fold } : keys + [fold]).joined(separator: ",")
        }
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let title {
                HStack(spacing: 8) {
                    if fold != nil {
                        // A section that folds: its title is the toggle, the chevron turns.
                        Button(action: toggle) {
                            HStack(spacing: 6) {
                                heading(title)
                                Image(systemName: "chevron.down")
                                    .font(.system(size: 10, weight: .semibold))
                                    .foregroundStyle(Palette.tertiary)
                                    .rotationEffect(.degrees(folded ? -90 : 0))
                            }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityValue(L10n.tr(folded ? "Collapsed" : "Expanded"))
                    } else {
                        heading(title)
                    }
                    Spacer(minLength: 0)
                    if !folded { accessory }
                }
                .padding(.leading, 2)
            }
            // No rows (`{}`): a footer-only note, without an empty card.
            if !folded, Content.self != EmptyView.self {
                VStack(alignment: .leading, spacing: 0) {
                    _VariadicView.Tree(HairlineRows()) { content }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(cardStyle == .filled ? Palette.surface : .clear, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                .overlay {
                    if cardStyle == .outlined {
                        RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Palette.border)
                    }
                }
            }
            if !folded, let footer {
                Text(L10n.tr(footer))
                    .font(.caption)
                    .foregroundStyle(Palette.secondary)
                    .lineSpacing(2)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 2)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func heading(_ title: String) -> some View {
        Text(L10n.tr(title))
            .font(.system(size: 15, weight: .semibold))
            .foregroundStyle(Palette.text)
            .accessibilityAddTraits(.isHeader)
    }
}

public extension CardSection where Accessory == EmptyView {
    init(_ title: String? = nil, footer: String? = nil, fold: String? = nil, @ViewBuilder content: () -> Content) {
        self.init(title, footer: footer, fold: fold, content: content) { EmptyView() }
    }
}

/// A 1-pixel separator line in the border color.
public struct Hairline: View {
    @Environment(\.displayScale) private var scale

    public init() {}

    public var body: some View {
        Rectangle().fill(Palette.border).frame(height: 1 / scale)
    }
}

/// Lays a section's rows out one under another with an inset hairline between each pair.
private struct HairlineRows: _VariadicView_MultiViewRoot {
    func body(children: _VariadicView.Children) -> some View {
        let inset: CGFloat = 16
        ForEach(children) { child in
            child
                .frame(maxWidth: .infinity, minHeight: 32, alignment: .leading)
                .padding(.horizontal, inset)
                .padding(.vertical, 14)
            if child.id != children.last?.id { Hairline().padding(.horizontal, inset) }
        }
    }
}

/// How `CardSection` draws its group: filled (Settings style), or outlined on the page like Grok
/// Bot's routine cards (the bot details sheet).
public enum CardStyle: Sendable {
    case filled, outlined
}

public extension EnvironmentValues {
    @Entry var cardStyle: CardStyle = .filled
}

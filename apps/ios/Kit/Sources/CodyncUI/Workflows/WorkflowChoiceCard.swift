import CodyncKit
import SwiftUI

/// The step row used by the new-workflow form and the editor. Icon, title, detail, optional number and source.
struct WorkflowChoiceCard: View {
    var symbol: String
    var title: String
    var detail: String
    var showsChevron: Bool = true
    var number: Int?
    var parentNumbers: [Int] = []
    var caption: String?

    var body: some View {
        HStack(spacing: 12) {
            if let number {
                Text("\(number)")
                    .font(.body.monospaced().weight(.semibold))
                    .foregroundStyle(Palette.text)
                    .frame(minWidth: 16, alignment: .trailing)
            }
            Image(systemName: symbol)
                .font(.system(size: 16, weight: .semibold))
                .foregroundStyle(Palette.text)
                .frame(width: 36, height: 36)
                .background(Palette.background, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            VStack(alignment: .leading, spacing: 2) {
                Text(L10n.tr(title))
                    .font(.body.weight(.medium))
                    .foregroundStyle(Palette.text)
                    .lineLimit(1)
                if !detail.isEmpty {
                    Text(L10n.tr(detail))
                        .font(.caption)
                        .foregroundStyle(Palette.secondary)
                        .lineLimit(2)
                }
                if !parentNumbers.isEmpty {
                    WorkflowSourceLine(numbers: parentNumbers)
                }
                if let caption {
                    Text(L10n.tr(caption))
                        .font(.caption)
                        .foregroundStyle(Palette.secondary)
                        .lineLimit(2)
                }
            }
            Spacer(minLength: 8)
            if showsChevron {
                Image(systemName: "chevron.right")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(Palette.tertiary)
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Palette.surface, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
    }
}

/// The full-width add control under a column of steps.
struct WorkflowAddStepButton: View {
    var action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                Image(systemName: "plus")
                    .font(.body.weight(.semibold))
                Text(L10n.tr("Add step"))
                    .font(.body.weight(.medium))
            }
            .foregroundStyle(Palette.text)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 14)
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        }
        .buttonStyle(.plain)
    }
}

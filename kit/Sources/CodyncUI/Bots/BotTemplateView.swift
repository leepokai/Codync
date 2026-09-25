import CodyncKit
import SwiftUI

/// A portable settings snapshot, with no conversation identity or runtime state.
struct BotTemplateView: View {
    @Environment(\.dismiss) private var dismiss
    @State private var copied = false
    private let template: BotDraft
    private let encoded: String?

    init(draft: BotDraft) {
        var template = draft
        template.id = nil
        template.pinned = nil
        template.hidden = nil
        self.template = template
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        encoded = (try? encoder.encode(template)).flatMap { String(data: $0, encoding: .utf8) }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            HStack {
                VStack(alignment: .leading, spacing: 5) {
                    Text("Create template").font(.title2.weight(.semibold))
                    Text(template.name).font(.subheadline).foregroundStyle(Palette.secondary)
                }
                Spacer()
                Button("Close", systemImage: "xmark") { dismiss() }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.plain)
                    .keyboardShortcut(.cancelAction)
            }
            Text("Copy this bot’s settings to reuse as a template. Conversation history and conversation ID are excluded.")
                .font(.callout)
                .foregroundStyle(Palette.secondary)
            ScrollView {
                Text(encoded ?? "Unable to create this template.")
                    .font(.system(size: 12, design: .monospaced))
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(16)
            }
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: 12))
            .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(Palette.border))
            HStack {
                Text("Includes the working folder and agent configuration.")
                    .font(.caption).foregroundStyle(Palette.secondary)
                Spacer()
                Button {
                    Pasteboard.copy(encoded)
                    copied = true
                } label: {
                    Label(copied ? "Copied" : "Copy template", systemImage: copied ? "checkmark" : "square.on.square")
                        .font(.system(size: 13, weight: .semibold))
                        .padding(.horizontal, 14).padding(.vertical, 9)
                        .foregroundStyle(Palette.onAccent)
                        .background(Palette.accentFill, in: RoundedRectangle(cornerRadius: 9))
                }
                .buttonStyle(.plain)
                .disabled(encoded == nil)
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding(24)
        .frame(width: 560, height: 540)
        .background(Palette.background)
    }
}

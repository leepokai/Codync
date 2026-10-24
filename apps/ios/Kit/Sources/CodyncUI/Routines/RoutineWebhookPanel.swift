import CodyncKit
import SwiftUI

/// A saved webhook routine's address and key: copy either, reveal or replace the key.
struct RoutineWebhookPanel: View {
    let botId: String
    let routineId: String
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var webhook: RoutineWebhook?
    @State private var error: String?
    @State private var showKey = false
    @State private var copied: String?
    @State private var confirmRotate = false
    @State private var rotating = false

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            if let webhook {
                Field(webhook.url == nil ? "Local URL" : "URL") {
                    value(webhook.url ?? webhook.localUrl, copy: "url")
                }
                Field("Key") {
                    value(showKey ? webhook.key : String(repeating: "•", count: 16), copy: "key") {
                        IconButton(showKey ? "Hide key" : "Show key", systemImage: showKey ? "eye.slash" : "eye") {
                            withAnimation(Motion.reduced(Motion.hover, reduceMotion)) { showKey.toggle() }
                        }
                        IconButton("Replace key", systemImage: "arrow.triangle.2.circlepath") { confirmRotate = true }
                            .disabled(rotating || model.isOffline)
                    }
                }
                Text(note(webhook))
                    .font(.caption).foregroundStyle(Palette.secondary).padding(.leading, 4)
                    .fixedSize(horizontal: false, vertical: true)
            } else if let error {
                Text(L10n.tr(error)).font(.caption).foregroundStyle(Palette.danger).padding(.leading, 4)
            } else {
                Text(L10n.tr("Loading webhook…")).font(.caption).foregroundStyle(Palette.secondary).padding(.leading, 4)
            }
        }
        .task(id: routineId) { await load(rotate: false) }
        .codyncDialog("Replace the key?", isPresented: $confirmRotate,
                      message: "Senders using the current key stop working until they get the new one.") {
            [DialogAction("Replace key", destructive: true) { Task { await load(rotate: true) } }]
        }
    }

    private func note(_ webhook: RoutineWebhook) -> String {
        guard webhook.url != nil else {
            return "The Codync cloud is off, so only this computer can send to it."
        }
        return "POST with Authorization: Bearer <key>. For GitHub, use content type application/json and the key as the secret. "
            + "Deliveries wait up to 72 hours while this computer is off. They pass through the Codync cloud, which can read them."
    }

    /// A selectable monospaced value with a copy button, plus any extra buttons.
    private func value(_ text: String, copy: String, @ViewBuilder extra: () -> some View = { EmptyView() }) -> some View {
        HStack(alignment: .center, spacing: 8) {
            Text(text)
                .font(.callout.monospaced())
                .lineLimit(2)
                .truncationMode(.middle)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
            extra()
            IconButton(copied == copy ? "Copied" : "Copy", systemImage: copied == copy ? "checkmark" : "doc.on.doc") {
                guard let webhook else { return }
                Pasteboard.copy(copy == "key" ? webhook.key : webhook.url ?? webhook.localUrl)
                withAnimation(Motion.reduced(Motion.hover, reduceMotion)) { copied = copy }
            }
        }
        .padding(.vertical, 8)
        .padding(.leading, 12)
        .padding(.trailing, 6)
        .background(Palette.bubbleUser, in: RoundedRectangle(cornerRadius: 12))
    }

    private func load(rotate: Bool) async {
        guard let client = model.client else { error = "Connect to this computer to see the webhook."; return }
        rotating = rotate
        defer { rotating = false }
        do {
            let value = try await client.routineWebhook(botId: botId, id: routineId, rotate: rotate)
            withAnimation(Motion.reduced(Motion.hover, reduceMotion)) {
                webhook = value
                if rotate { copied = nil; showKey = true }
            }
            error = nil
        } catch { self.error = error.localizedDescription }
    }
}

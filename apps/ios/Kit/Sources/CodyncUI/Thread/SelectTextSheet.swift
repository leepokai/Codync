import CodyncKit
import SwiftUI
import UIKit

/// A message's text as it was written, selectable: Reply on a selection quotes that part in place of the whole message.
struct SelectTextSheet: View {
    let text: String
    /// Starts a reply to the selected part.
    let reply: (String) -> Void

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader("Select Text")
            SelectableText(text: text, reply: reply)
        }
        .background(Palette.background)
    }
}

/// SwiftUI's `Text` doesn't say what is selected in it, so this text is a `UITextView`, whose edit menu gains Reply.
private struct SelectableText: UIViewRepresentable {
    let text: String
    let reply: (String) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(reply: reply) }

    func makeUIView(context: Context) -> UITextView {
        let view = UITextView()
        view.isEditable = false
        view.backgroundColor = .clear
        view.font = .preferredFont(forTextStyle: .body)
        view.adjustsFontForContentSizeCategory = true
        view.textColor = UIColor(Palette.text)
        view.textContainerInset = UIEdgeInsets(top: 4, left: 16, bottom: 24, right: 16)
        view.showsVerticalScrollIndicator = false
        view.delegate = context.coordinator
        view.accessibilityIdentifier = "select-text"
        return view
    }

    func updateUIView(_ view: UITextView, context: Context) {
        if view.text != text { view.text = text }
        context.coordinator.reply = reply
    }

    @MainActor final class Coordinator: NSObject, UITextViewDelegate {
        var reply: (String) -> Void

        init(reply: @escaping (String) -> Void) { self.reply = reply }

        func textView(_ textView: UITextView, editMenuForTextIn range: NSRange, suggestedActions: [UIMenuElement]) -> UIMenu? {
            guard range.length > 0, let selected = Range(range, in: textView.text) else { return nil }
            let part = String(textView.text[selected])
            let reply = UIAction(title: L10n.tr("Reply"), image: UIImage(systemName: "arrowshape.turn.up.left")) { [weak self] _ in
                self?.reply(part)
            }
            return UIMenu(children: [reply] + suggestedActions)
        }
    }
}

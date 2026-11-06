import CodyncKit
import SwiftUI
import UIKit

// MARK: - Dialogs

/// A button in a Codync dialog.
public struct DialogAction: Identifiable {
    public let id = UUID()
    public var title: String
    public var destructive: Bool
    public var action: () -> Void

    public init(_ title: String, destructive: Bool = false, action: @escaping () -> Void) {
        self.title = title
        self.destructive = destructive
        self.action = action
    }
}

/// A text box above a dialog's actions (a name to type), focused as the dialog opens.
public struct DialogField {
    public var placeholder: String
    public var text: Binding<String>

    public init(_ placeholder: String, text: Binding<String>) {
        self.placeholder = placeholder
        self.text = text
    }
}

public extension View {
    /// A centered card over a dimmed screen with the actions and Cancel:
    /// the replacement for `confirmationDialog` and `alert`. A `field` sits above the actions;
    /// Return in it runs the first action.
    /// Set `inPlace` only at the navigation root to keep underlying glass unchanged.
    func codyncDialog(_ title: String, isPresented: Binding<Bool>, message: String? = nil, field: DialogField? = nil,
                      cancel: String? = "Cancel", inPlace: Bool = false,
                      actions: @escaping () -> [DialogAction]) -> some View {
        modifier(CodyncDialog(title: title, message: message, field: field, cancel: cancel, inPlace: inPlace,
                              isPresented: isPresented, actions: actions))
            .reviewInterruption(active: isPresented.wrappedValue)
    }
}

private struct CodyncDialog: ViewModifier {
    let title: String
    let message: String?
    let field: DialogField?
    let cancel: String?
    /// Use only above the navigation container so the scrim also covers its toolbar.
    let inPlace: Bool
    @Binding var isPresented: Bool
    let actions: () -> [DialogAction]
    /// Counts the dismissals: each opening of the in-place card is a fresh card, so one that closed
    /// and reopened before a redraw can't keep its `closed` flag.
    @State private var opening = 0
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    func body(content: Content) -> some View {
        if inPlace {
            content
                .allowsHitTesting(!isPresented)
                .accessibilityHidden(isPresented)
                .overlay {
                    if isPresented {
                        DialogCard(title: title, message: message, field: field, cancel: cancel, actions: actions()) {
                            isPresented = false
                            opening += 1
                        }
                        .id(opening)
                        .ignoresSafeArea(.container)
                        .transition(.opacity)
                        .onAppear {
                            // A field takes the focus on the next update: don't race it with a keyboard drop.
                            guard field == nil else { return }
                            UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder),
                                                             to: nil, from: nil, for: nil)
                        }
                    }
                }
                .animation(Motion.reduced(Motion.fade, reduceMotion), value: isPresented)
        } else {
            content.codyncOverlay(isPresented: $isPresented) { close in
                DialogCard(title: title, message: message, field: field, cancel: cancel, actions: actions(), dismiss: close)
            }
        }
    }
}

private struct DialogCard: View {
    let title: String
    let message: String?
    let field: DialogField?
    let cancel: String?
    let actions: [DialogAction]
    let dismiss: () -> Void
    @FocusState private var typing: Bool
    /// Set once the card begins to close: a second tap or Return during the fade runs nothing.
    @State private var closed = false

    var body: some View {
        ZStack {
            Color.black.opacity(0.35).ignoresSafeArea()
                .onTapGesture { if cancel != nil { close() } }
            VStack(spacing: 14) {
                VStack(spacing: 6) {
                    Text(L10n.tr(title)).font(.headline).foregroundStyle(Palette.text)
                    if let message, !message.isEmpty {
                        Text(L10n.tr(message)).font(.subheadline).foregroundStyle(Palette.secondary)
                    }
                }
                .multilineTextAlignment(.center)
                if let field {
                    TextField(L10n.tr(field.placeholder), text: field.text)
                        .textFieldStyle(.plain)
                        .font(.body)
                        .foregroundStyle(Palette.text)
                        .submitLabel(.done)
                        .focused($typing)
                        .onSubmit { if let first = actions.first { run(first) } }
                        .padding(.horizontal, 14)
                        .padding(.vertical, 12)
                        .background(Palette.background, in: Capsule())
                        .task { typing = true }
                }
                VStack(spacing: 8) {
                    ForEach(actions) { action in
                        Button {
                            run(action)
                        } label: {
                            Text(L10n.tr(action.title)).frame(maxWidth: .infinity)
                        }
                        .buttonStyle(DialogButtonStyle(destructive: action.destructive, prominent: true))
                    }
                    if let cancel {
                        Button(action: close) { Text(L10n.tr(cancel)).frame(maxWidth: .infinity) }
                            .buttonStyle(DialogButtonStyle(destructive: false, prominent: false))
                            .keyboardShortcut(.cancelAction)
                    }
                }
            }
            .padding(18)
            .frame(maxWidth: 320)
            .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
            .shadow(color: .black.opacity(0.25), radius: 24, y: 10)
            .padding(24)
        }
        .accessibilityAddTraits(.isModal)
    }

    private func close() {
        guard !closed else { return }
        closed = true
        dismiss()
    }

    private func run(_ action: DialogAction) {
        guard !closed else { return }
        close()
        action.action()
    }
}

private struct DialogButtonStyle: ButtonStyle {
    let destructive: Bool
    let prominent: Bool

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.body.weight(.medium))
            .foregroundStyle(destructive ? Color.white : (prominent ? Palette.onAccent : Palette.text))
            .padding(.vertical, 12)
            .background(destructive ? Palette.danger : (prominent ? Palette.accentFill : Palette.bubbleUser), in: Capsule())
            .opacity(configuration.isPressed ? 0.85 : 1)
            .scaleEffect(configuration.isPressed ? Motion.pressScale : 1)
            .animation(Motion.press, value: configuration.isPressed)
            .contentShape(Capsule())
    }
}

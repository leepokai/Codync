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

public extension View {
    /// A centered card over a dimmed screen with the actions and Cancel:
    /// the replacement for `confirmationDialog` and `alert`.
    /// Set `inPlace` only at the navigation root to keep underlying glass unchanged.
    func codyncDialog(_ title: String, isPresented: Binding<Bool>, message: String? = nil,
                      cancel: String? = "Cancel", inPlace: Bool = false,
                      actions: @escaping () -> [DialogAction]) -> some View {
        modifier(CodyncDialog(title: title, message: message, cancel: cancel, inPlace: inPlace,
                              isPresented: isPresented, actions: actions))
    }
}

private struct CodyncDialog: ViewModifier {
    let title: String
    let message: String?
    let cancel: String?
    /// Use only above the navigation container so the scrim also covers its toolbar.
    let inPlace: Bool
    @Binding var isPresented: Bool
    let actions: () -> [DialogAction]
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    func body(content: Content) -> some View {
        if inPlace {
            content
                .allowsHitTesting(!isPresented)
                .accessibilityHidden(isPresented)
                .overlay {
                    if isPresented {
                        DialogCard(title: title, message: message, cancel: cancel, actions: actions()) {
                            isPresented = false
                        }
                        .ignoresSafeArea(.container)
                        .transition(.opacity)
                        .onAppear {
                            UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder),
                                                             to: nil, from: nil, for: nil)
                        }
                    }
                }
                .animation(Motion.reduced(Motion.fade, reduceMotion), value: isPresented)
        } else {
            content.codyncOverlay(isPresented: $isPresented) { close in
                DialogCard(title: title, message: message, cancel: cancel, actions: actions(), dismiss: close)
            }
        }
    }
}

private struct DialogCard: View {
    let title: String
    let message: String?
    let cancel: String?
    let actions: [DialogAction]
    let dismiss: () -> Void

    var body: some View {
        ZStack {
            Color.black.opacity(0.35).ignoresSafeArea()
                .onTapGesture { if cancel != nil { dismiss() } }
            VStack(spacing: 14) {
                VStack(spacing: 6) {
                    Text(title).font(.headline).foregroundStyle(Palette.text)
                    if let message, !message.isEmpty {
                        Text(message).font(.subheadline).foregroundStyle(Palette.secondary)
                    }
                }
                .multilineTextAlignment(.center)
                VStack(spacing: 8) {
                    ForEach(actions) { action in
                        Button {
                            dismiss()
                            action.action()
                        } label: {
                            Text(action.title).frame(maxWidth: .infinity)
                        }
                        .buttonStyle(DialogButtonStyle(destructive: action.destructive, prominent: true))
                    }
                    if let cancel {
                        Button(action: dismiss) { Text(cancel).frame(maxWidth: .infinity) }
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

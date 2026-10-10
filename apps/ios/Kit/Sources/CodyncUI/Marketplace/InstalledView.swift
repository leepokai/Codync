import AuthenticationServices
import CodyncKit
import SwiftUI

struct InstalledView: View {
    let back: () -> Void
    @Environment(BotStore.self) private var model
    @Environment(\.webAuthenticationSession) private var webAuthenticationSession
    @Environment(\.openURL) private var openURL
    @State private var removing: Removal?
    @State private var signingIn: String?

    private struct Removal: Identifiable {
        let id: String
        let name: String
        let isSkill: Bool
    }

    var body: some View {
        VStack(spacing: 0) {
            ScreenHeader {
                BackButton(action: back).keyboardShortcut(.cancelAction)
            } title: {
                Text("Installed").font(.body.weight(.semibold)).foregroundStyle(Palette.text)
            } trailing: {
                EmptyView()
            }
            list
        }
        .background(Palette.background)
    }

    private var list: some View {
        CardForm {
            if model.installedConnectors.isEmpty && model.installedSkills.isEmpty {
                EmptyState(icon: "shippingbox", title: "Nothing installed", detail: "Connectors and skills you add show up here.")
            }
            if !model.installedConnectors.isEmpty {
                CardSection("Connectors") {
                    ForEach(model.installedConnectors) { c in
                        InstalledRow(title: c.name, subtitle: c.needsSignIn ? "Sign in so bots can use it" : c.command ?? c.url ?? c.description) {
                            if c.kind == "composio" {
                                AppLogo(url: c.logo, name: c.name, size: 32)
                            } else {
                                ServiceLogo(website: nil, name: c.name, registryName: c.registryName, size: 32)
                            }
                        } accessory: {
                            if signingIn == c.id {
                                Spinner()
                            } else if c.needsSignIn {
                                Button("Sign in") { signIn(c.id) }
                                    .buttonStyle(PrimaryButtonStyle())
                                    .controlSize(.small)
                            }
                        } remove: {
                            removing = Removal(id: c.id, name: c.name, isSkill: false)
                        }
                    }
                }
            }
            if !model.installedSkills.isEmpty {
                CardSection("Skills") {
                    ForEach(model.installedSkills) { s in
                        InstalledRow(title: s.name, subtitle: s.description) {
                            SkillGlyph().frame(width: 32, height: 32)
                        } accessory: {
                            EmptyView()
                        } remove: {
                            removing = Removal(id: s.id, name: s.name, isSkill: true)
                        }
                    }
                }
            }
        }
        .codyncDialog(
            "Remove \(removing?.name ?? "")?",
            isPresented: Binding(get: { removing != nil }, set: { if !$0 { removing = nil } }),
            message: removing.map { $0.isSkill ? "Bots stop using this skill." : "Bots lose this connector, and the keys saved for it are deleted." }
        ) {
            guard let r = removing else { return [] }
            return [DialogAction("Remove", destructive: true) {
                Task {
                    do {
                        if r.isSkill { try await model.removeSkill(r.id) } else { try await model.removeConnector(r.id) }
                    } catch {
                        model.lastError = error.localizedDescription
                    }
                }
            }]
        }
    }

    private func signIn(_ id: String) {
        signingIn = id
        Task {
            do {
                try await ConnectorSignInFlow(model: model, webAuthenticationSession: webAuthenticationSession, openURL: openURL).signIn(id)
            } catch {
                model.lastError = error.localizedDescription
            }
            signingIn = nil
        }
    }
}

/// A quiet "nothing here" message (replaces `ContentUnavailableView`).
private struct EmptyState: View {
    let icon: String
    let title: String
    let detail: String

    var body: some View {
        VStack(spacing: 8) {
            Image(systemName: icon).font(.system(size: 28)).foregroundStyle(Palette.tertiary).accessibilityHidden(true)
            Text(title).font(.body.weight(.semibold)).foregroundStyle(Palette.text)
            Text(detail).font(.subheadline).foregroundStyle(Palette.secondary).multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 40)
    }
}

private struct InstalledRow<Icon: View, Accessory: View>: View {
    let title: String
    let subtitle: String
    @ViewBuilder let icon: Icon
    @ViewBuilder let accessory: Accessory
    let remove: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            icon
            VStack(alignment: .leading, spacing: 2) {
                Text(title).foregroundStyle(Palette.text).lineLimit(1)
                Text(subtitle).font(.caption).foregroundStyle(Palette.secondary).lineLimit(1)
            }
            Spacer(minLength: 12)
            accessory
            Button("Remove \(title)", systemImage: "trash", role: .destructive, action: remove)
                .labelStyle(.iconOnly)
                .buttonStyle(.plain)
                .foregroundStyle(Palette.secondary)
                .frame(minWidth: 44, minHeight: 44)
                .contentShape(Rectangle())
                .help("Remove")
        }
        .help(subtitle)
    }
}

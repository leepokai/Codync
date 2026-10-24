import AuthenticationServices
import CodyncKit
import SwiftUI

struct InstallConnectorSheet: View {
    let item: MarketConnector
    var requestId: String? = nil
    let done: () -> Void
    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss
    @Environment(\.webAuthenticationSession) private var webAuthenticationSession
    @Environment(\.openURL) private var openURL
    @State private var optionId = ""
    @State private var values: [String: String] = [:]
    @State private var installed: InstalledConnector?
    @State private var started = false
    @State private var saving = false
    @State private var error: String?

    private var option: MarketConnector.InstallOption? {
        item.options.first { $0.id == optionId } ?? item.options.first
    }

    private var missing: Bool {
        option?.inputs.contains { $0.required && (values[$0.name] ?? $0.default ?? "").isEmpty } ?? true
    }

    /// After a try that failed, Add with nothing typed tries the connector it saved again.
    private var retrying: Bool { installed != nil && !values.values.contains { !$0.isEmpty } }

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader("Add connector") {
                if saving {
                    Spinner()
                } else {
                    IconButton("Add", systemImage: "plus") { install() }
                        .disabled(missing && !retrying)
                }
            }
            form
        }
        .background(Palette.background)
        .task {
            guard !started else { return }
            started = true
            if item.options.count == 1 && item.options[0].inputs.isEmpty { install() }
        }
        .onDisappear { values.removeAll() }
    }

    private var form: some View {
        CardForm {
            CardSection {
                VStack(alignment: .leading, spacing: 6) {
                    Text(item.title).font(.title3.weight(.semibold))
                    if let d = item.description { Text(d).foregroundStyle(Palette.secondary) }
                    if let w = item.website, let url = URL(string: w) {
                        WebLink(w, url: url).font(.footnote).lineLimit(1)
                    }
                }
                .padding(.vertical, 4)
            }
            if item.options.count > 1 {
                CardSection("Runs") {
                    ChoiceList(selection: $optionId, options: item.options.map { o in
                        (id: o.id, label: o.kind == "remote"
                            ? L10n.tr("Hosted by {name}").replacingOccurrences(of: "{name}", with: item.title)
                            : L10n.tr("On {name} ({kind})").replacingOccurrences(of: "{name}", with: model.hostName).replacingOccurrences(of: "{kind}", with: o.kind),
                         detail: nil)
                    })
                }
            }
            if let option {
                CardSection("Setup", footer: L10n.tr("Keys are saved on {name} only.").replacingOccurrences(of: "{name}", with: model.hostName)) {
                    if option.inputs.isEmpty {
                        Text(option.kind == "remote"
                            ? L10n.tr("No keys needed here. If {name} wants you to sign in, its sign-in page opens next.").replacingOccurrences(of: "{name}", with: item.title)
                            : L10n.tr("No setup needed."))
                            .foregroundStyle(Palette.secondary)
                    }
                    ForEach(option.inputs) { input in
                        VStack(alignment: .leading, spacing: 4) {
                            Group {
                                if input.secret {
                                    SecureField(input.name, text: binding(input))
                                } else {
                                    TextField(input.name, text: binding(input), prompt: Text(input.default ?? input.placeholder ?? input.name))
                                }
                            }
                            .plainTextInput()
                            if let d = input.description {
                                Text(d + (input.required ? "" : L10n.tr(" (optional)"))).font(.caption).foregroundStyle(Palette.secondary)
                            }
                        }
                    }
                }
            }
            if let error {
                CardSection { Text(L10n.tr(error)).foregroundStyle(Palette.danger) }
            }
        }
        .textFieldStyle(.plain)
        .onAppear { optionId = item.options.first?.id ?? "" }
    }

    private func binding(_ input: MarketConnector.Input) -> Binding<String> {
        Binding(get: { values[input.name] ?? "" }, set: { values[input.name] = $0 })
    }

    private func install() {
        guard let option else { return }
        saving = true
        error = nil
        Task {
            do {
                guard let client = model.client else { return }
                // The host keeps the connector a try saved unless new values or another way to run it come in.
                let c = try await model.installConnector(item, option: option.id, inputs: values)
                installed = c
                values.removeAll()
                if c.needsSignIn {
                    try await ConnectorSignInFlow(model: model, webAuthenticationSession: webAuthenticationSession, openURL: openURL).signIn(c.id)
                }
                if let requestId {
                    try await client.finishConnectionRequest(requestId, connectorId: c.id)
                } else {
                    try await client.verifyConnector(c.id)
                }
                await model.refreshPlugins()
                done()
                dismiss()
            } catch {
                self.error = error.localizedDescription
            }
            saving = false
        }
    }
}

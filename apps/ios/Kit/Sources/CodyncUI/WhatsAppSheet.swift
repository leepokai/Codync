import CodyncKit
import CoreImage
import CoreImage.CIFilterBuiltins
import SwiftUI
import UIKit

/// The computer's linked WhatsApp number. WhatsApp has no official client API: the number becomes
/// a linked device, and a separate number is safer because WhatsApp can block one it sees as
/// automated. The computer runs the link; a phone only shows its state and lets someone link or
/// unlink. The computer is the `BotStore` in the environment.
public struct WhatsAppSheet: View {
    @Environment(BotStore.self) private var store

    @State private var link: WhatsAppLink?
    @State private var botChoice = ""
    @State private var jid = ""
    @State private var phone = ""
    @State private var error: String?
    @State private var notice: String?
    @State private var busy = false

    public init() {}

    public var body: some View {
        VStack(spacing: 0) {
            ModalHeader("WhatsApp")
            CardForm {
                stateSection
                botSection
                if link?.state == "connected" {
                    allowedSection
                }
                if let error {
                    Text(L10n.tr(error)).font(.caption).foregroundStyle(Palette.danger)
                }
                if let notice {
                    Text(L10n.tr(notice)).font(.caption).foregroundStyle(Palette.secondary)
                }
            }
        }
        .background(Palette.background)
        // The link changes on its own while a QR code is scanned, so it is read again while this is open.
        .task {
            while !Task.isCancelled {
                let next = try? await store.whatsapp()
                link = next
                // A save in flight owns the choice until it lands.
                if !busy { botChoice = next?.bot ?? "" }
                try? await Task.sleep(for: .milliseconds(1500))
            }
        }
        .animation(Motion.layout, value: link)
    }

    private var stateSection: some View {
        CardSection(footer: "WhatsApp does not support this kind of client officially and can block a number it sees as automated. A separate number is safer.") {
            Text(L10n.tr(WhatsApp.stateText(link?.state ?? ""))).foregroundStyle(Palette.text)
            if let code = link?.qr, link?.state == "qr" {
                VStack(alignment: .leading, spacing: 10) {
                    Text(L10n.tr("On your phone, open WhatsApp → Linked devices → Link a device, then scan this code."))
                        .foregroundStyle(Palette.secondary)
                    QrImage(text: code)
                        .frame(maxWidth: .infinity)
                }
            }
            if let code = link?.pairCode, link?.state == "pairing" {
                VStack(alignment: .leading, spacing: 8) {
                    Text(L10n.tr("On your phone, open WhatsApp → Linked devices → Link a device, choose Link with phone number, then type this code."))
                        .foregroundStyle(Palette.secondary)
                    Text(WhatsApp.groupedCode(code))
                        .font(.title2.monospaced())
                        .foregroundStyle(Palette.text)
                        // Read one character at a time, as the phone shows the field.
                        .accessibilityLabel(code.map(String.init).joined(separator: " "))
                    Button("Copy code") {
                        UIPasteboard.general.string = code
                        notice = "Code copied."
                    }
                    .buttonStyle(.secondary)
                    .disabled(busy)
                }
            }
            if link?.state == "connected" {
                VStack(alignment: .leading, spacing: 4) {
                    Text(link?.name ?? link?.me ?? "").foregroundStyle(Palette.text)
                    Text(L10n.tr("Messages from your allowed chats reach the bot you pick. A workflow can also start from a mention."))
                        .font(.footnote)
                        .foregroundStyle(Palette.secondary)
                }
            }
            if link?.state != "starting", link?.state != "connected", link?.state != "qr", link?.state != "pairing" {
                VStack(alignment: .leading, spacing: 8) {
                    TextField("5215555555555", text: $phone)
                        .textFieldStyle(.plain)
                        .plainTextInput()
                        .keyboardType(.phonePad)
                        .padding(.horizontal, 14)
                        .frame(height: 38)
                        .background(Palette.bubbleUser, in: Capsule())
                    Button("Link with a code") {
                        act { link = try await store.whatsappPair(phone: phone); link = try? await store.whatsapp() }
                    }
                    .buttonStyle(.primary)
                    .disabled(busy || phone.trimmingCharacters(in: .whitespaces).isEmpty)
                    Button("Scan a QR code") {
                        act { try await store.whatsappConnect(); link = try? await store.whatsapp() }
                    }
                    .buttonStyle(.secondary)
                    .disabled(busy)
                }
            }
            if let state = link?.state, ["connected", "qr", "starting", "pairing"].contains(state) {
                Button(state == "connected" ? "Unlink" : "Cancel") {
                    act { try await store.whatsappLogout(); link = try? await store.whatsapp() }
                }
                .buttonStyle(.secondary)
                .disabled(busy)
            }
            if let hostError = link?.error {
                Text(hostError).font(.caption).foregroundStyle(Palette.danger)
            }
        }
    }

    private var botSection: some View {
        CardSection {
            ValueRow("Bot", detail: "Messages from your allowed chats reach the bot you pick. A workflow can also start from a mention.") {
                ChoicePicker(selection: $botChoice, options: [(id: "", label: L10n.tr("No bot"))] + bots,
                             fill: Palette.bubbleUser)
            }
        }
        .onChange(of: botChoice) { _, choice in
            guard choice != (link?.bot ?? "") else { return }
            act { try await store.whatsappSetBot(choice.isEmpty ? nil : choice); link = try? await store.whatsapp() }
        }
    }

    /// Anyone can write while this is empty; the owner always can.
    private var allowedSection: some View {
        CardSection("Allowed chats", footer: "Anyone can write while this is empty. The owner always can.") {
            ForEach(link?.allowed ?? [], id: \.self) { entry in
                ValueRow(entry) {
                    Button("Remove") {
                        act {
                            _ = try await store.whatsappSetAllowed(entry, allowed: false)
                            link = try? await store.whatsapp()
                        }
                    }
                    .buttonStyle(.secondary)
                    .disabled(busy)
                }
            }
            VStack(alignment: .leading, spacing: 8) {
                TextField("5215555555555@s.whatsapp.net", text: $jid)
                    .textFieldStyle(.plain)
                    .plainTextInput()
                    .textInputAutocapitalization(.never)
                    .padding(.horizontal, 14)
                    .frame(height: 38)
                    .background(Palette.bubbleUser, in: Capsule())
                Button("Allow") {
                    let value = jid.trimmingCharacters(in: .whitespaces)
                    act {
                        _ = try await store.whatsappSetAllowed(value, allowed: true)
                        jid = ""
                        link = try? await store.whatsapp()
                    }
                }
                .buttonStyle(.secondary)
                .disabled(busy || jid.trimmingCharacters(in: .whitespaces).isEmpty)
            }
            .padding(.vertical, 2)
        }
    }

    private var bots: [(id: String, label: String)] {
        store.bots.values.filter { !$0.deleted }
            .sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }.map { ($0.id, $0.name) }
    }

    private func act(_ work: @escaping @MainActor () async throws -> Void) {
        guard !busy else { return }
        busy = true
        error = nil
        notice = nil
        Task {
            do {
                try await work()
            } catch {
                self.error = error.localizedDescription
            }
            busy = false
        }
    }
}

/// The QR code a linked-device scan reads, with a white plate so dark themes keep it readable.
private struct QrImage: View {
    let text: String

    var body: some View {
        if let image = Self.make(text) {
            Image(uiImage: image)
                .interpolation(.none)
                .resizable()
                .scaledToFit()
                .frame(width: 232, height: 232)
                .padding(10)
                .background(Color.white, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                .accessibilityLabel(L10n.tr("WhatsApp QR code"))
        }
    }

    private static func make(_ text: String) -> UIImage? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(text.utf8)
        filter.correctionLevel = "M"
        guard let output = filter.outputImage?.transformed(by: CGAffineTransform(scaleX: 10, y: 10)),
              let image = CIContext().createCGImage(output, from: output.extent) else { return nil }
        return UIImage(cgImage: image)
    }
}

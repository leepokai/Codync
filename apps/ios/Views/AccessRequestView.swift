import CodyncKit
import CodyncUI
import SwiftUI

/// Asking a computer in the account for access (spec §4.2 B): once both sides committed,
/// the same 6-digit code shows here and on the computer. The user approves there only if they match.
struct AccessRequestView: View {
    let computer: CloudComputer
    @Environment(AccountStore.self) private var accounts
    @Environment(\.dismissModal) private var dismiss
    @State private var error: String?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// A code was shown: when it goes away without an approval, the request is over (denied or expired).
    @State private var sawTicket: Bool
    /// Why it ended, taken from `accounts.lastError` when the code went: shown here, so the computers
    /// list under this sheet doesn't say it again.
    @State private var outcome: String?
    @State private var requestId: String?
    @State private var confirmRenewal: Bool
    @State private var requesting = false
    @State private var renewRequest = false

    init(computer: CloudComputer, pending: AccessTicket?) {
        self.computer = computer
        _sawTicket = State(initialValue: pending != nil)
        _requestId = State(initialValue: pending?.requestId)
        _confirmRenewal = State(initialValue: computer.isGranted && pending == nil)
    }

    private var ticket: AccessTicket? { accounts.pendingAccess[computer.computerId] }
    private var approved: Bool { requestId != nil && accounts.approvedAccess[computer.computerId] == requestId }

    var body: some View {
        VStack(spacing: 24) {
            ComputerBadge(Computer(id: computer.computerId, name: computer.name, signKey: computer.signKey, device: computer.device), size: 64)
                .padding(.top, 32)

            if confirmRenewal {
                Text("This phone already has approval. If access no longer works, request a new approval on the computer.")
                    .foregroundStyle(Palette.secondary)
                    .multilineTextAlignment(.center)
                Text("This replaces this phone's current approval. The computer must be online to show the new code.")
                    .font(.footnote)
                    .foregroundStyle(Palette.secondary)
                    .multilineTextAlignment(.center)
                Button("Request new approval") {
                    withAnimation(Motion.reduced(Motion.layout, reduceMotion)) {
                        renewRequest = true
                        confirmRenewal = false
                    }
                }.buttonStyle(.secondary)
            } else if approved {
                Label("\(computer.name) approved this iPhone", systemImage: "checkmark.circle.fill")
                    .font(.headline)
                    .foregroundStyle(Palette.added)
            } else if let ticket {
                VStack(spacing: 10) {
                    Text(L10n.tr("Check the code on \(computer.name)"))
                        .font(.headline)
                        .foregroundStyle(Palette.text)
                    Text(spaced(ticket.code))
                        .font(.system(size: 52, weight: .semibold, design: .monospaced))
                        .foregroundStyle(Palette.text)
                        .textSelection(.enabled)
                        .accessibilityLabel("Code \(ticket.code.map(String.init).joined(separator: " "))")
                    Text(L10n.tr("Approve only if the computer shows exactly this code. If it doesn't, deny it there: someone may be trying to get in."))
                        .font(.subheadline)
                        .foregroundStyle(Palette.secondary)
                        .multilineTextAlignment(.center)
                }
                HStack(spacing: 8) {
                    Spinner()
                    Text(L10n.tr("Waiting for approval…")).foregroundStyle(Palette.secondary)
                    IconButton("Cancel request", systemImage: "xmark.circle.fill") {
                        Task {
                            await accounts.cancelAccess(computer.computerId)
                            dismiss()
                        }
                    }
                }
                .font(.subheadline)
            } else if let error {
                Text(error)
                    .foregroundStyle(Palette.danger)
                    .multilineTextAlignment(.center)
                Button(L10n.tr("Try again")) { withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { self.error = nil } }
                    .buttonStyle(.secondary)
            } else if sawTicket {
                Text(outcome ?? "\(computer.name) didn't approve this request.")
                    .foregroundStyle(Palette.secondary)
                    .multilineTextAlignment(.center)
                Button(L10n.tr("Ask again")) {
                    withAnimation(Motion.reduced(Motion.layout, reduceMotion)) {
                        outcome = nil
                        sawTicket = false
                    }
                }
                .buttonStyle(.secondary)
            } else {
                VStack(spacing: 10) {
                    Spinner()
                    Text(computer.isOnline ? "Asking \(computer.name)…" : "\(computer.name) is offline. Turn it on to continue.")
                        .font(.subheadline)
                        .foregroundStyle(Palette.secondary)
                        .multilineTextAlignment(.center)
                }
                .task { await request() }
            }
            Spacer()
        }
        .padding(24)
        .frame(maxWidth: .infinity)
        .background(Palette.background)
        // Closing keeps waiting in the background; the computer shows up once approved.
        .safeAreaInset(edge: .top, spacing: 0) { ModalHeader("Access") }
        .onChange(of: ticket?.requestId) { old, id in
            if let id {
                requestId = id
                sawTicket = true
            } else if old != nil, !approved {
                outcome = accounts.lastError
                accounts.lastError = nil
            }
        }
        .onChange(of: approved) { _, done in
            guard done else { return }
            Task {
                try? await Task.sleep(for: .seconds(1))
                dismiss()
            }
        }
    }

    /// Runs until the computer answered with its nonce; the code then shows through `pendingAccess`.
    /// Closing the sheet leaves the request, and this wait, running to its end with the result
    /// ignored; the code then shows in the banner.
    private func request() async {
        guard !requesting else { return }
        requesting = true
        defer { requesting = false }
        do {
            try await accounts.requestAccess(computer.computerId, renew: renewRequest)
            renewRequest = false
        } catch is CancellationError {
            return
        } catch {
            guard !Task.isCancelled else { return }
            self.error = error.localizedDescription
        }
    }

    private func spaced(_ code: String) -> String {
        guard code.count == 6 else { return code }
        return "\(code.prefix(3)) \(code.suffix(3))"
    }
}

import CodyncKit
import CodyncUI
import SwiftUI

/// Asking a computer in the account for access (spec §4.2 B): once both sides committed,
/// the same 6-digit code shows here and on the computer. The user approves there only if they match.
struct AccessRequestView: View {
    let computer: CloudComputer
    @Environment(AccountStore.self) private var accounts
    @Environment(\.dismiss) private var dismiss
    @State private var error: String?
    /// A code was shown: when it goes away without an approval, the request is over (denied or expired).
    @State private var sawTicket: Bool

    init(computer: CloudComputer, pending: Bool) {
        self.computer = computer
        _sawTicket = State(initialValue: pending)
    }

    private var ticket: AccessTicket? { accounts.pendingAccess[computer.computerId] }
    private var approved: Bool { accounts.store(for: computer.computerId) != nil }

    var body: some View {
        VStack(spacing: 24) {
            ComputerBadge(Computer(id: computer.computerId, name: computer.name, signKey: computer.signKey, device: computer.device), size: 64)
                .padding(.top, 32)

            if approved {
                Label("\(computer.name) approved this iPhone", systemImage: "checkmark.circle.fill")
                    .font(.headline)
                    .foregroundStyle(Palette.added)
            } else if let ticket {
                VStack(spacing: 10) {
                    Text("Check the code on \(computer.name)")
                        .font(.headline)
                        .foregroundStyle(Palette.text)
                    Text(spaced(ticket.code))
                        .font(.system(size: 52, weight: .semibold, design: .monospaced))
                        .foregroundStyle(Palette.text)
                        .textSelection(.enabled)
                        .accessibilityLabel("Code \(ticket.code.map(String.init).joined(separator: " "))")
                    Text("Approve only if the computer shows exactly this code. If it doesn't, deny it there: someone may be trying to get in.")
                        .font(.subheadline)
                        .foregroundStyle(Palette.secondary)
                        .multilineTextAlignment(.center)
                }
                HStack(spacing: 8) {
                    ProgressView()
                    Text("Waiting for approval…").foregroundStyle(Palette.secondary)
                }
                .font(.subheadline)
            } else if let error {
                Text(error)
                    .foregroundStyle(Palette.danger)
                    .multilineTextAlignment(.center)
                Button("Try again") { self.error = nil }
                    .buttonStyle(.bordered)
            } else if sawTicket {
                Text(accounts.lastError ?? "\(computer.name) didn't approve this request.")
                    .foregroundStyle(Palette.secondary)
                    .multilineTextAlignment(.center)
                Button("Ask again") {
                    accounts.lastError = nil
                    sawTicket = false
                }
                .buttonStyle(.bordered)
            } else {
                VStack(spacing: 10) {
                    ProgressView()
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
        .navigationTitle("Access")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                // Closing keeps waiting in the background; the computer shows up once approved.
                Button("Close", systemImage: "xmark") { dismiss() }.labelStyle(.iconOnly)
            }
        }
        .onChange(of: ticket?.requestId) { _, id in if id != nil { sawTicket = true } }
        .onChange(of: approved) { _, done in
            guard done else { return }
            Task {
                try? await Task.sleep(for: .seconds(1))
                dismiss()
            }
        }
    }

    /// Runs until the computer answered with its nonce; closing the sheet cancels it.
    private func request() async {
        do {
            _ = try await accounts.requestAccess(computer.computerId)
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

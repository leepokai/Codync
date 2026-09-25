import CodyncKit
import CodyncUI
import SwiftUI

struct AccountSwitcherButton: View {
    @Environment(AccountSession.self) private var account

    var body: some View {
        Button { account.showSwitcher = true } label: {
            HStack(spacing: 4) {
                AccountAvatar(url: account.avatarURL, size: 30)
                Image(systemName: "chevron.down")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(Palette.secondary)
            }
        }
        .accessibilityLabel("Switch account")
        .accessibilityValue(account.email ?? "Local pairing")
    }
}

struct AccountSwitcherView: View {
    @Environment(AccountSession.self) private var account
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        Form {
            Section("Accounts") {
                if account.accounts.isEmpty {
                    Label("Local pairing", systemImage: "person.crop.circle")
                        .foregroundStyle(Palette.secondary)
                }
                ForEach(account.accounts) { item in
                    Button {
                        Task { await account.switchAccount(item) }
                    } label: {
                        HStack(spacing: 12) {
                            AccountAvatar(url: item.avatarURL, size: 34)
                            Text(item.email)
                                .lineLimit(1)
                                .foregroundStyle(Palette.text)
                            Spacer(minLength: 8)
                            if item.id == account.userID {
                                Image(systemName: "checkmark")
                                    .foregroundStyle(Palette.text)
                                    .accessibilityLabel("Current account")
                            }
                        }
                    }
                    .disabled(account.isBusy)
                }
                if !account.isSignedIn || account.supportsMultipleAccounts {
                    Button {
                        Task { await account.signIn() }
                    } label: {
                        Label(account.isSignedIn ? "Add Google account" : "Continue with Google", systemImage: "person.crop.circle.badge.plus")
                    }
                    .disabled(account.isBusy || !account.isConfigured)
                }
            }

            Section {
                NavigationLink {
                    SettingsView()
                } label: {
                    Label("Computers & settings", systemImage: "desktopcomputer")
                }
            } footer: {
                Text("Computer connections are saved separately for each account on this iPhone. Pair a computer to see its bots.")
            }

            if account.isSignedIn {
                Section {
                    Button("Sign out of this account", role: .destructive) {
                        Task { await account.signOut() }
                    }
                    .disabled(account.isBusy)
                } footer: {
                    if !account.supportsMultipleAccounts {
                        Text("Sign out to use a different Google account.")
                    }
                }
            }

            if !account.isConfigured {
                Section {
                    Text("Account sign-in isn't configured in this build. Local pairing is available.")
                        .foregroundStyle(Palette.secondary)
                }
            }
            if let message = account.errorMessage {
                Section {
                    Text(message).foregroundStyle(Palette.danger)
                }
            }
        }
        .scrollContentBackground(.hidden)
        .background(Palette.background)
        .navigationTitle("Accounts")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Close", systemImage: "xmark") { dismiss() }
                    .labelStyle(.iconOnly)
            }
            if account.isBusy {
                ToolbarItem(placement: .topBarTrailing) { ProgressView() }
            }
        }
    }
}

private struct AccountAvatar: View {
    let url: URL?
    let size: CGFloat

    var body: some View {
        AsyncImage(url: url) { image in
            image.resizable().scaledToFill()
        } placeholder: {
            Image(systemName: "person.crop.circle.fill")
                .resizable()
                .foregroundStyle(Palette.secondary)
        }
        .frame(width: size, height: size)
        .clipShape(Circle())
        .accessibilityHidden(true)
    }
}

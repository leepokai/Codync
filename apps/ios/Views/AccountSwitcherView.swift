import CodyncKit
import CodyncUI
import SwiftUI

struct AccountSwitcherButton: View {
    @Environment(AccountSession.self) private var account

    var body: some View {
        Button { account.showSwitcher = true } label: {
            HStack(spacing: 4) {
                AccountAvatar(url: account.avatarURL, email: account.email, size: 30)
                Image(systemName: "chevron.down")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(Palette.secondary)
            }
        }
        .accessibilityLabel("Switch account")
        .accessibilityValue(account.email ?? "Local pairing")
    }
}

/// Who you're signed in as, the other accounts on this iPhone, and the way to the computers.
struct AccountSwitcherView: View {
    @Environment(AppStore.self) private var app
    @Environment(AccountSession.self) private var account
    @Environment(AccountStore.self) private var accounts
    @State private var showSettings = false
    @State private var confirmSignOut = false
    @State private var route = ConnectionRoute.current

    var body: some View {
        CardForm {
            profile
                .frame(maxWidth: .infinity)
                .padding(.vertical, 8)

            if account.isSignedIn, account.supportsMultipleAccounts || account.accounts.count > 1 {
                switcher
            }

            CardSection("Connection") {
                RoutePicker(selection: Binding(get: { route }, set: { setRoute($0) }))
            }

            CardSection {
                Button { showSettings = true } label: {
                    RowLabel("Computers", systemImage: "desktopcomputer", chevron: true)
                }
                .buttonStyle(.plain)
                if account.isSignedIn {
                    Button { confirmSignOut = true } label: {
                        RowLabel("Sign out", systemImage: "rectangle.portrait.and.arrow.right", tint: Palette.danger)
                    }
                    .buttonStyle(.plain)
                    .disabled(account.isBusy)
                }
            }

            if let message = account.errorMessage {
                Label(message, systemImage: "exclamationmark.triangle.fill")
                    .font(.footnote)
                    .foregroundStyle(Palette.danger)
                    .transition(.opacity)
            }
        }
        .safeAreaInset(edge: .top, spacing: 0) {
            ModalHeader("Account") {
                if account.isBusy {
                    ThinkingOrb(state: .connecting, size: 20)
                        .accessibilityHidden(false).accessibilityLabel("Signing in")
                        .transition(.opacity)
                }
            }
            .background(Palette.background)
            .animation(Motion.fade, value: account.isBusy)
        }
        .animation(Motion.layout, value: account.userID)
        .animation(Motion.fade, value: account.errorMessage)
        .hidesSystemNavigationBar()
        .navigationDestination(isPresented: $showSettings) { SettingsView(pushed: true) }
        .codyncDialog("Sign out of \(account.email ?? "this account")?", isPresented: $confirmSignOut,
                      message: "This iPhone forgets the account's computers. Your bots stay on them.") {
            [DialogAction("Sign out", destructive: true) { Task { await app.signOut() } }]
        }
    }

    /// Saved for this iPhone; every computer reconnects over the new route right away.
    private func setRoute(_ new: ConnectionRoute) {
        withAnimation(Motion.morph) { route = new }
        ConnectionRoute.current = new
        for computer in accounts.computers { accounts.store(for: computer.id)?.restartStream() }
    }

    /// The big picture: your face and address when signed in, the Google button when not.
    @ViewBuilder private var profile: some View {
        if account.isSignedIn {
            VStack(spacing: 10) {
                AccountAvatar(url: account.avatarURL, email: account.email, size: 76)
                    .overlay(alignment: .bottomTrailing) {
                        Image("google")
                            .resizable()
                            .frame(width: 16, height: 16)
                            .padding(5)
                            .background(Palette.background, in: Circle())
                            .offset(x: 2, y: 2)
                            .accessibilityHidden(true)
                    }
                Text(account.email ?? "")
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(Palette.text)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            .transition(.opacity.combined(with: .scale(scale: 0.96)))
        } else {
            VStack(spacing: 16) {
                ZStack {
                    Circle().fill(Palette.bubbleAgent).frame(width: 76, height: 76)
                    Image(systemName: "person.fill")
                        .font(.system(size: 32))
                        .foregroundStyle(Palette.tertiary)
                }
                .accessibilityHidden(true)
                if account.isConfigured {
                    GoogleSignInButton()
                } else {
                    Label("QR pairing only in this build", systemImage: "qrcode")
                        .font(.footnote)
                        .foregroundStyle(Palette.secondary)
                }
            }
            .transition(.opacity.combined(with: .scale(scale: 0.96)))
        }
    }

    /// Every account on this iPhone as a face; tap one to switch, + to add another.
    private var switcher: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 14) {
                ForEach(account.accounts) { item in
                    let current = item.id == account.userID
                    Button { Task { await account.switchAccount(item) } } label: {
                        AccountAvatar(url: item.avatarURL, email: item.email, size: 44)
                            .padding(3)
                            .overlay(Circle().strokeBorder(current ? Palette.accent : .clear, lineWidth: 2))
                    }
                    .buttonStyle(PressScale())
                    .disabled(account.isBusy || current)
                    .accessibilityLabel(item.email)
                    .accessibilityAddTraits(current ? .isSelected : [])
                }
                if account.supportsMultipleAccounts {
                    Button { Task { await account.signIn() } } label: {
                        Image(systemName: "plus")
                            .font(.system(size: 18, weight: .medium))
                            .foregroundStyle(Palette.secondary)
                            .frame(width: 50, height: 50)
                            .background(Palette.bubbleAgent, in: Circle())
                    }
                    .buttonStyle(PressScale())
                    .disabled(account.isBusy)
                    .accessibilityLabel("Add Google account")
                }
            }
            .padding(.horizontal, 4)
            .frame(maxWidth: .infinity)
        }
        .scrollBounceBehavior(.basedOnSize)
    }
}

/// Three routes as icons; one line under them says what the chosen one does.
private struct RoutePicker: View {
    @Binding var selection: ConnectionRoute

    private func info(_ route: ConnectionRoute) -> (icon: String, title: String, detail: String) {
        switch route {
        case .automatic: ("wand.and.sparkles", "Auto", "Wi-Fi or Tailscale when it answers, Cloudflare otherwise.")
        case .direct: ("point.3.connected.trianglepath.dotted", "Direct", "Wi-Fi and Tailscale only. Nothing goes through the cloud.")
        case .relay: ("cloud", "Cloudflare", "Always through the encrypted relay, from anywhere.")
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                ForEach(ConnectionRoute.allCases, id: \.self) { route in
                    let item = info(route)
                    let on = route == selection
                    Button { selection = route } label: {
                        VStack(spacing: 6) {
                            Image(systemName: item.icon).font(.system(size: 20))
                            Text(item.title).font(.caption.weight(.medium))
                        }
                        .foregroundStyle(on ? Palette.onAccent : Palette.secondary)
                        .frame(maxWidth: .infinity, minHeight: 64)
                        .background(on ? Palette.accentFill : Palette.background, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(PressScale())
                    .accessibilityLabel(item.title)
                    .accessibilityHint(item.detail)
                    .accessibilityAddTraits(on ? .isSelected : [])
                }
            }
            Text(info(selection).detail)
                .font(.caption)
                .foregroundStyle(Palette.secondary)
                .contentTransition(.opacity)
        }
    }
}

/// Google's mark plus the one line it needs; used wherever sign-in is offered.
struct GoogleSignInButton: View {
    @Environment(AccountSession.self) private var account

    var body: some View {
        Button { Task { await account.signIn() } } label: {
            ZStack {
                HStack(spacing: 10) {
                    Image("google").resizable().frame(width: 20, height: 20)
                    Text("Continue with Google")
                }
                .opacity(account.isBusy ? 0 : 1)
                if account.isBusy { Spinner(size: 18) }
            }
            .font(.headline)
            .frame(maxWidth: .infinity, minHeight: 50)
        }
        .buttonStyle(.secondary)
        .disabled(account.isBusy)
        .animation(Motion.fade, value: account.isBusy)
    }
}

/// An icon and a short name, with a chevron when it leads somewhere.
private struct RowLabel: View {
    let title: String
    let systemImage: String
    var tint: Color = Palette.text
    var chevron = false

    init(_ title: String, systemImage: String, tint: Color = Palette.text, chevron: Bool = false) {
        self.title = title
        self.systemImage = systemImage
        self.tint = tint
        self.chevron = chevron
    }

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: systemImage)
                .font(.system(size: 17))
                .frame(width: 26)
            Text(title)
            Spacer(minLength: 8)
            if chevron {
                Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(Palette.tertiary)
            }
        }
        .foregroundStyle(tint)
        .contentShape(Rectangle())
    }
}

/// The account's photo; its first letter on a tinted circle until (or unless) the photo loads.
struct AccountAvatar: View {
    let url: URL?
    var email: String?
    let size: CGFloat

    var body: some View {
        AsyncImage(url: url) { phase in
            if let image = phase.image {
                image.resizable().scaledToFill()
            } else {
                ZStack {
                    Circle().fill(Palette.bubbleUser)
                    if let letter = email?.first {
                        Text(String(letter).uppercased())
                            .font(.system(size: size * 0.42, weight: .semibold))
                            .foregroundStyle(Palette.text)
                    } else {
                        Image(systemName: "person.fill")
                            .font(.system(size: size * 0.42))
                            .foregroundStyle(Palette.tertiary)
                    }
                }
            }
        }
        .frame(width: size, height: size)
        .clipShape(Circle())
        .accessibilityHidden(true)
    }
}

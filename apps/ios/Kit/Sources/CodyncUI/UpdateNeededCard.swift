import CodyncKit
import SwiftUI

/// Updates this app (opens its App Store page).
public struct AppUpdateAction: Sendable {
    let action: @MainActor @Sendable () -> Void

    public init(_ action: @escaping @MainActor @Sendable () -> Void) { self.action = action }

    @MainActor public func callAsFunction() { action() }
}

public extension EnvironmentValues {
    @Entry var appUpdate: AppUpdateAction?
}

/// Says which side has to update before this app and a computer can work together, and offers
/// the update where this app can start it. Shown instead of the composer and above the
/// computer's bots (docs/reference/compatibility.md).
public struct UpdateNeededCard: View {
    let store: BotStore
    let mismatch: VersionMismatch
    @Environment(\.appUpdate) private var appUpdate
    @Environment(AppUpdates.self) private var appUpdates: AppUpdates?

    public init(store: BotStore, mismatch: VersionMismatch) {
        self.store = store
        self.mismatch = mismatch
    }

    public var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: "arrow.down.circle")
                .font(.title3)
                .foregroundStyle(Palette.warning)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 6) {
                Text(title).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.text)
                Text(detail)
                    .font(.footnote)
                    .foregroundStyle(Palette.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                if let action {
                    Button(action.title, action: action.run)
                        .buttonStyle(.primary)
                        .disabled(inReview)
                        .padding(.top, 4)
                }
            }
            Spacer(minLength: 0)
        }
        .padding(14)
        .background(Palette.surface, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .accessibilityElement(children: .contain)
    }

    var title: String { UpdateNeededText.title(mismatch, host: store.hostName) }
    var detail: String {
        let text = UpdateNeededText.detail(mismatch, host: store.hostName, hostVersion: store.hostVersion?.version,
                                           // With a button here, instructions for that computer would only confuse.
                                           os: store.hello?.os, instructions: action == nil)
        return inReview ? "\(text) That version is still in App Store review; it'll be there soon." : text
    }

    /// The App Store doesn't have the app this computer needs yet. An unknown store version
    /// keeps the button: the App Store page itself shows what's there.
    private var inReview: Bool {
        guard case let .updateApp(minimum) = mismatch, let store = appUpdates?.store else { return false }
        return AppVersion.isBelow(store, minimum)
    }

    private var action: (title: String, run: () -> Void)? {
        switch mismatch {
        case .updateApp:
            guard let appUpdate else { return nil }
            return ("Update Codync", { appUpdate() })
        case .updateHost:
            // The computer updates itself; the card says how.
            return nil
        }
    }
}

/// The words every client uses for a version mismatch (the desktop app and the terminal client
/// mirror them).
public enum UpdateNeededText {
    public static func title(_ mismatch: VersionMismatch, host: String) -> String {
        switch mismatch {
        case .updateApp: "Update this app"
        case .updateHost: "Update Codync on \(host)"
        }
    }

    public static func detail(_ mismatch: VersionMismatch, host: String, hostVersion: String?, os: String?,
                              instructions: Bool = true) -> String {
        switch mismatch {
        case let .updateApp(minimum):
            let runs = hostVersion.map { "\(host) runs Codync \($0) and" } ?? host
            return "\(runs) needs this app to be \(minimum) or newer. Your chats are safe on the computer."
        case let .updateHost(version, minimum):
            let how = switch os {
            case "macos": "Open Codync on that Mac and choose Check for Updates."
            case "linux": "Run codync-host update there."
            default: "Update Codync on that computer."
            }
            let needs = "\(host) runs Codync \(version); this app needs \(minimum) or newer."
            return instructions ? "\(needs) \(how)" : needs
        }
    }
}

/// What the App Store has, asked now and then (iPhone): the update notice greys out its button
/// while the needed version is in review, and a newer release gets a dismissible reminder.
@MainActor
@Observable
public final class AppUpdates {
    /// The version live in the App Store; nil until asked or when the lookup fails.
    public private(set) var store: String?
    @ObservationIgnored private var askedAt: Date?
    private var dismissedApp: String? = UserDefaults.standard.string(forKey: "dismissedAppUpdate")

    public init() {}

    /// Asks the App Store at most every few hours (foreground, launch).
    public func refresh() async {
        if let askedAt, Date().timeIntervalSince(askedAt) < 6 * 3600 { return }
        askedAt = Date()
        guard let version = try? await AppStoreRelease.latestVersion() else { return }
        if version != store { Motion.animate { store = version } }
    }

    /// A newer app release to remind about, unless dismissed.
    public var newerApp: String? {
        UpdateReminder.app(current: AppVersion.current, store: store, dismissed: dismissedApp)
    }

    public func dismissApp() {
        Motion.animate { dismissedApp = store }
        UserDefaults.standard.set(store, forKey: "dismissedAppUpdate")
    }
}

/// A quiet, dismissible note that a newer release is out (everything still works).
public struct UpdateReminderCard: View {
    let title: String
    let detail: String
    let actionTitle: String?
    let action: () -> Void
    let dismiss: () -> Void

    public init(title: String, detail: String, actionTitle: String? = nil,
                action: @escaping () -> Void = {}, dismiss: @escaping () -> Void) {
        self.title = title
        self.detail = detail
        self.actionTitle = actionTitle
        self.action = action
        self.dismiss = dismiss
    }

    public var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: "arrow.down.circle.fill")
                .font(.title3.weight(.medium))
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(Palette.text)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 6) {
                Text(title).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.text)
                Text(detail)
                    .font(.footnote)
                    .foregroundStyle(Palette.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                if let actionTitle {
                    Button(actionTitle, action: action)
                        .buttonStyle(.secondary)
                        .padding(.top, 4)
                }
            }
            Spacer(minLength: 0)
            IconButton("Dismiss", systemImage: "xmark", action: dismiss)
                .padding(.top, -6)
                .padding(.trailing, -6)
        }
        .padding(14)
        .background(Palette.surface, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .accessibilityElement(children: .contain)
    }
}

/// The App Store has a newer app (iPhone; needs `AppUpdates` in the environment).
public struct AppUpdateReminder: View {
    @Environment(AppUpdates.self) private var appUpdates: AppUpdates?
    @Environment(\.appUpdate) private var appUpdate

    public init() {}

    public var body: some View {
        if let appUpdates, let version = appUpdates.newerApp {
            UpdateReminderCard(title: "Codync \(version) is available",
                               detail: "You have \(AppVersion.current).",
                               actionTitle: appUpdate == nil ? nil : "Update",
                               action: { appUpdate?() },
                               dismiss: appUpdates.dismissApp)
                .padding(.vertical, 4)
                .transition(.opacity)
        }
    }
}

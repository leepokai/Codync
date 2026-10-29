import Foundation

/// What the interface calls the computer's relay. The official clouds stay "Cloudflare".
/// Any other cloud URL is that person's private relay, named by its host.
public enum RelayCopy {
    private static let official = ["api.codync.dev", "dev-api.codync.dev"]

    /// Host (and a non-default port) when `cloud` is a private relay. Nil for the official clouds.
    public static func host(of cloud: URL?) -> String? {
        guard let cloud, let name = cloud.host?.lowercased() else { return nil }
        if official.contains(name), cloud.port == nil { return nil }
        // `URL.host` drops an IPv6 address's brackets; the other clients keep them.
        let shown = name.contains(":") ? "[\(name)]" : name
        if let port = cloud.port { return "\(shown):\(port)" }
        return shown
    }

    /// The connection menu sentence. Official clouds keep the existing Cloudflare phrases.
    public static func routeTitle(_ route: ConnectionRoute, cloud: URL?) -> String {
        guard let host = host(of: cloud) else {
            return switch route {
            case .automatic: "Auto: Wi-Fi or Tailscale, then Cloudflare"
            case .cloudflareFirst: "Cloudflare, then Wi-Fi or Tailscale"
            case .directOnly: "Wi-Fi or Tailscale only, never Cloudflare"
            }
        }
        return switch route {
        case .automatic: "Auto: Wi-Fi or Tailscale, then Private relay (\(host))"
        case .cloudflareFirst: "Private relay (\(host)), then Wi-Fi or Tailscale"
        case .directOnly: "Wi-Fi or Tailscale only, never Private relay (\(host))"
        }
    }

    /// The route's short name, shown on the Connection row; the menu keeps `routeTitle`.
    public static func routeShortTitle(_ route: ConnectionRoute, cloud: URL?) -> String {
        switch route {
        case .automatic: "Auto"
        case .cloudflareFirst: host(of: cloud) == nil ? "Cloudflare first" : "Private relay first"
        case .directOnly: "Wi-Fi/Tailscale only"
        }
    }

    /// The short suffix after a bot count: ` · Cloudflare` or ` · Private relay (host)`.
    public static func detailSuffix(route: HostRoute?, cloud: URL?) -> String {
        switch route {
        case .direct: " · Wi-Fi/Tailscale"
        case .relay: host(of: cloud).map { " · Private relay (\($0))" } ?? " · Cloudflare"
        case nil: ""
        }
    }

    public static func connected(cloud: URL?) -> String {
        host(of: cloud).map { "Connected through Private relay (\($0))" } ?? "Connected through Cloudflare"
    }

    public static func through(cloud: URL?) -> String {
        host(of: cloud).map { "Through Private relay (\($0))" } ?? "Through Cloudflare"
    }

    public static func pairing(cloud: URL?) -> String {
        host(of: cloud).map {
            "Connects on Wi-Fi, over Tailscale, or from anywhere through Private relay (\($0)), on its own. End-to-end encrypted."
        } ?? "Connects on Wi-Fi, over Tailscale, or from anywhere through Cloudflare, on its own. End-to-end encrypted."
    }

    /// The pairing page's line. Parsing stays here so a view only renders the sentence.
    public static func pairing(pasted: String) -> String {
        pairing(cloud: Pairing(string: pasted)?.computer.cloud)
    }
}

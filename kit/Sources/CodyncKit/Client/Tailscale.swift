import Darwin
import Foundation

/// Tailscale gives every device an address in 100.64.0.0/10 on a VPN (`utun`) interface.
public enum Tailscale {
    public static let downloadURL = URL(string: "https://tailscale.com/download")!

    /// A host address that only works over Tailscale (its IP range or MagicDNS name).
    public static func isAddress(_ url: String) -> Bool {
        guard let host = URL(string: url)?.host() else { return false }
        if host.hasSuffix(".ts.net") { return true }
        let o = host.split(separator: ".").compactMap { UInt8($0) }
        return o.count == 4 && isRange(o[0], o[1])
    }

    /// Tailscale is connected on this device right now.
    public static var isConnected: Bool {
        var head: UnsafeMutablePointer<ifaddrs>?
        guard getifaddrs(&head) == 0 else { return false }
        defer { freeifaddrs(head) }
        var next = head
        while let ifa = next?.pointee {
            next = ifa.ifa_next
            // Only VPN interfaces: some carriers hand out 100.64/10 addresses on cellular too.
            guard String(cString: ifa.ifa_name).hasPrefix("utun"),
                  let addr = ifa.ifa_addr, addr.pointee.sa_family == UInt8(AF_INET) else { continue }
            let ip = addr.withMemoryRebound(to: sockaddr_in.self, capacity: 1) { $0.pointee.sin_addr.s_addr }
            let o = withUnsafeBytes(of: ip) { Array($0) }
            if isRange(o[0], o[1]) { return true }
        }
        return false
    }

    private static func isRange(_ a: UInt8, _ b: UInt8) -> Bool { a == 100 && (64..<128).contains(b) }
}

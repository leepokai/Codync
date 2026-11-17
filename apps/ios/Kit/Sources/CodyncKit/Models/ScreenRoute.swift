/// Which way a remote screen attempt goes, by the computer's Connection choice: TURN is the screen's way through
/// Cloudflare (docs/features/remote-screen.md). The other way gets the next attempt when one fails.
public enum ScreenRoute {
    /// Whether attempt number `failedAttempts` asks for TURN. `channelOnRelay`: chat itself reaches the computer
    /// through a relay. `hasTurn`: the computer is in an account on the Codync cloud; a computer in no account and a
    /// private relay have none.
    public static func asksForTurn(channelOnRelay: Bool, route: ConnectionRoute, hasTurn: Bool, failedAttempts: Int) -> Bool {
        let later = failedAttempts % 2 == 1
        switch route {
        case .directOnly: return false
        case .cloudflareFirst: return hasTurn ? !later : channelOnRelay
        case .automatic: return channelOnRelay || (hasTurn && later)
        }
    }
}

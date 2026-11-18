import CodyncKit
import Foundation

/// Hands a share to its chat the way the app sends: connects to the bot's computer, uploads the files, then sends the
/// message. Text alone waits in the relay mailbox while the computer is offline; files need it online.
enum ShareDelivery {
    enum Outcome { case sent, queued }

    struct Failure: Error {
        let kind: AnalyticsEvent.ShareSent.Failure
        /// An interface phrase (`L10n`).
        let message: String

        static let retry = "Couldn't send. Try again."
        static let unreachable = Failure(kind: .unreachable, message: HostError.unreachable.localizedDescription)
    }

    /// `pasted`: what came from the other app rather than the user, which memory never takes as their own words.
    /// `nonce`: the share's, the same on every attempt. `sent`: bytes of the files uploaded so far.
    @MainActor static func deliver(_ text: String, files: [OutgoingFile], pasted: [String], nonce: String,
                                   to recipient: ShareRecipient, in context: SharedStore.Context,
                                   sent: @escaping @Sendable (Int) -> Void) async throws(Failure) -> Outcome {
        let transport = try await connect(recipient, in: context)
        defer { Task { await transport.shutdown() } }
        var state = LinkState.connecting
        for await current in transport.states() {
            state = current
            break
        }
        switch state {
        case .ready:
            try await send(text, files: files, pasted: pasted, nonce: nonce, to: recipient.botId, over: transport, sent: sent)
            return .sent
        case .hostOffline:
            guard files.isEmpty else { throw Failure(kind: .offline, message: "Files go out once your computer is online.") }
            do { try await transport.enqueue(botId: recipient.botId, text: text, clientNonce: nonce, pasted: pasted) }
            catch { throw Failure(kind: .send, message: Failure.retry) }
            return .queued
        case .connecting, .failed:
            throw Failure.unreachable
        case let .unauthorized(message):
            throw Failure(kind: .unauthorized, message: message)
        }
    }

    @MainActor private static func connect(_ recipient: ShareRecipient, in context: SharedStore.Context) async throws(Failure) -> ChannelTransport {
        guard let computer = context.computers.first(where: { $0.id == recipient.computerId }),
              let identity = try? DeviceIdentity.load(context: context) else {
            throw Failure(kind: .unauthorized, message: "Open Codync and connect a computer first.")
        }
        do {
            return try await HostConnector.connect(computer, identity: identity)
        } catch let HostError.unauthorized(message) {
            throw Failure(kind: .unauthorized, message: message)
        } catch HostError.upgradeRequired {
            throw Failure(kind: .unauthorized, message: HostError.upgradeRequired.localizedDescription)
        } catch {
            throw Failure.unreachable
        }
    }

    private static func send(_ text: String, files: [OutgoingFile], pasted: [String], nonce: String, to botId: String,
                             over transport: ChannelTransport, sent: @escaping @Sendable (Int) -> Void) async throws(Failure) {
        let client = HostClient(transport: transport)
        let ids: [String]
        do { ids = try await client.upload(botId: botId, files: files, sent: sent) }
        catch { throw Failure(kind: .upload, message: Failure.retry) }
        do {
            _ = try await client.send(botId: botId, text: text, clientNonce: nonce, attachments: ids.isEmpty ? nil : ids, pasted: pasted)
        } catch {
            throw Failure(kind: .send, message: Failure.retry)
        }
    }
}

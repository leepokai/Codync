import Foundation

public extension HostClient {
    func screenCandidates(session: String) -> AsyncThrowingStream<ScreenCandidate, Error> {
        let source = transport.stream(.screenCandidates(session))
        return AsyncThrowingStream(bufferingPolicy: .bufferingOldest(ScreenCandidate.maxCount + 2)) { c in
            let task = Task {
                do {
                    for try await data in source {
                        let event = try JSONDecoder().decode(ScreenCandidate.self, from: data)
                        try event.validate()
                        if case .dropped = c.yield(event) {
                            throw HostError.http(400, "Screen candidate stream overflow.")
                        }
                    }
                    c.finish()
                } catch { c.finish(throwing: error) }
            }
            c.onTermination = { _ in task.cancel() }
        }
    }

    func screenCandidate(session: String, candidate: ScreenCandidate) async throws {
        try candidate.validate()
        struct Body: Encodable { var session: String; var candidate: ScreenCandidate }
        let _: Empty = try await call("screenCandidate", Body(session: session, candidate: candidate))
    }
}

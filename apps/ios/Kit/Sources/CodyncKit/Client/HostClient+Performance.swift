public extension HostClient {
    func recordPerformance(_ measurement: PhonePerformance) async throws {
        let _: Empty = try await call("recordPhonePerformance", measurement)
    }
}

public extension Usage {
    /// The agents in `order`, a move still on its way to the host; any it doesn't name follow in the host's order.
    func services(in order: [String]?) -> [UsageService] {
        let all = services ?? []
        guard let order else { return all }
        return order.compactMap { id in all.first { $0.id == id } } + all.filter { !order.contains($0.id) }
    }

    /// On, with nothing to show: signed out, or nothing to report.
    func hasNoReading(_ service: UsageService) -> Bool {
        service.shown && !providers.contains { $0.id == service.id }
    }
}

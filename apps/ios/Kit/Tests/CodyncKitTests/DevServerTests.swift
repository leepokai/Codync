import Foundation
import Testing
@testable import CodyncKit

private func server(_ status: DevServer.Status = .running, name: String = "web", ports: [DevServer.Port] = [],
                    managed: Bool = true) -> DevServer {
    DevServer(id: "a", name: name, command: "npm run dev", folder: "/site", status: status, ports: ports, startedAt: 1,
              exitCode: nil, managed: managed)
}

private func port(_ number: Int, remote: [String] = []) -> DevServer.Port {
    DevServer.Port(port: number, local: "http://localhost:\(number)", remote: remote)
}

private let tailscale = "http://100.64.0.1:5173"
private let lan = "http://192.168.1.20:5173"
private let reachable = port(5173, remote: [lan])

@Suite struct DevServerTests {
    @Test func theTitleShowsNothingWithoutALiveServer() {
        #expect(DevServerSummary([]) == .none)
        #expect(DevServerSummary([server(.stopped)]) == .none)
    }

    @Test func theTitleShowsThePortAndHowManyMoreRun() {
        #expect(DevServerSummary([server(ports: [port(5173)])]) == .running(":5173"))
        let three = [server(name: "watch"), server(ports: [port(5173)]), server(ports: [port(8787)])]
        #expect(DevServerSummary(three) == .running(":5173 +2"))
    }

    @Test func theTitleNamesAServerWithoutAPort() {
        #expect(DevServerSummary([server(name: "watch")]) == .running("watch"))
    }

    @Test func runningOutranksStartingWhichOutranksFailed() {
        #expect(DevServerSummary([server(.failed), server(ports: [port(1)])]) == .running(":1"))
        #expect(DevServerSummary([server(.failed), server(.starting)]) == .starting)
        #expect(DevServerSummary([server(.failed), server(.stopped)]) == .failed)
    }

    @Test func aRowSaysItsPortsOrItsState() {
        #expect(server(ports: [port(5173), port(24678)]).rowValue == ":5173 :24678")
        #expect(server().rowValue == "Running")
        #expect(server(.starting).rowValue == "Starting")
        #expect(server(.stopped).rowValue == "Stopped")
        #expect(server(.failed).rowValue == "Failed to start")
    }

    @Test func tappingOpensWhatAPhoneCanReachAndLogsWhatEnded() {
        #expect(server(ports: [reachable]).tap == .open("http://192.168.1.20:5173"))
        // The first port listens only on the computer; the second doesn't.
        #expect(server(ports: [port(1), reachable]).tap == .open("http://192.168.1.20:5173"))
        #expect(server(ports: [port(5173)]).tap == nil)
        #expect(server().tap == .logs)
        #expect(server(.starting).tap == .logs)
        #expect(server(.stopped).tap == .logs)
        #expect(server(.failed).tap == .logs)
        #expect(server(.stopped, managed: false).tap == nil)
        // A stopped server's last address is no longer an address.
        #expect(server(.stopped, ports: [reachable]).tap == .logs)
    }

    @Test func theMenuOffersOnlyWhatApplies() {
        #expect(server(ports: [reachable]).menuRows == [.open, .copyAddress, .logs, .restart, .stop])
        #expect(server(ports: [port(5173)]).menuRows == [.onlyOnComputer, .logs, .restart, .stop])
        #expect(server().menuRows == [.logs, .restart, .stop])
        #expect(server(.starting).menuRows == [.logs, .stop])
        #expect(server(.stopped).menuRows == [.logs, .start, .remove])
        #expect(server(.failed).menuRows == [.logs, .start, .remove])
    }

    @Test func aTapOpensTheFirstAddressAndSeveralAreEachARowOfTheMenu() {
        let two = server(ports: [port(5173, remote: [tailscale, lan])])
        #expect(two.tap == .open(tailscale))
        #expect(two.remoteAddresses == [tailscale, lan])
        #expect(two.menuRows == [.openAddress(tailscale), .openAddress(lan), .copyAddress, .logs, .restart, .stop])
        // The first port listens only on the computer; the second doesn't.
        let second = server(ports: [port(1), port(5173, remote: [tailscale, lan])])
        #expect(Array(second.menuRows.prefix(2)) == [.openAddress(tailscale), .openAddress(lan)])
        // Copy address copies the first.
        #expect(two.remoteAddress == tailscale)
        #expect(server(.stopped, ports: [port(5173, remote: [tailscale, lan])]).menuRows == [.logs, .start, .remove])
    }

    @Test func aRowNamesAnAddressWithoutItsScheme() {
        #expect(DevServer.addressTitle(lan) == "192.168.1.20:5173")
    }

    @Test func aServerLeftInTheBotsShellCanOnlyBeOpenedOrStopped() {
        #expect(server(ports: [reachable], managed: false).menuRows == [.open, .copyAddress, .startedByShell, .stop])
        #expect(server(ports: [port(5173)], managed: false).menuRows == [.onlyOnComputer, .startedByShell, .stop])
    }

    @Test func aBotDecodesItsServersAndAnOlderHostsWithoutThem() throws {
        let json = """
        {"id":"b","devServers":[{"id":"a","name":"web","command":"npm run dev","folder":null,"status":"running",
        "ports":[{"port":5173,"local":"http://localhost:5173","remote":["http://192.168.1.20:5173"]}],
        "startedAt":null,"exitCode":null,"managed":true},
        {"id":"c","name":"w","command":"w","status":"sleeping","ports":[],"managed":false}]}
        """
        let bot = try JSONDecoder().decode(Bot.self, from: Data(json.utf8))
        #expect(bot.devServers.first?.remoteAddress == "http://192.168.1.20:5173")
        #expect(bot.devServers.last?.status == .stopped)
        #expect(try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"b"}"#.utf8)).devServers.isEmpty)
    }

    @Test func theEventNamesWhatWasDoneAndNothingElse() {
        #expect(AnalyticsEvent.devServerAction(.open).name == "dev_server_action")
        #expect(AnalyticsEvent.devServerAction(.copy).properties == ["action": .string("copy")])
        #expect(AnalyticsEvent.voiceNotePlayed(.load).name == "voice_note_played")
        #expect(AnalyticsEvent.voiceNotePlayed(nil).properties == ["outcome": .string("played")])
        #expect(AnalyticsEvent.voiceNotePlayed(.playback).properties == ["outcome": .string("failed"), "error": .string("playback")])
    }
}

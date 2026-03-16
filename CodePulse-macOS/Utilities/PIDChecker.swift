import Foundation

enum PIDChecker {
    static func isAlive(pid: Int) -> Bool {
        kill(Int32(pid), 0) == 0
    }
}

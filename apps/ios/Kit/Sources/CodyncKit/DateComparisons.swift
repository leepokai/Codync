import Foundation

// Xcode 27.2's SDK back-deploys Date's <= and >= (before: iOS 27.2). Every direct comparison, ours and
// a package's alike, then compiles to "if the OS is at least 27.2, call Foundation's copy, else compare
// here", and that call has no nil check. An iOS 27.2 beta seed that lacks the symbol jumped to address 0,
// which killed the app at launch once an account was signed in (#350).
//
// These carry the mangled names of Foundation's two operators, so the linker binds every such call in
// this binary here and imports nothing weakly. They compare as the back-deployed copies do. Only the test
// calls them by name. They stay public because Xcode prelinks a package target into one object, which hides
// anything less from the other targets, and static methods of Date so a direct comparison in this module
// still compiles (a global function fails, on the 27.2 SDK only).
//
// ponytail: these two symbols only. tools/check-ios-weak-imports.py checks each release's archive for the
// next weak import from Foundation, not from other frameworks. Delete this once the deployment target
// reaches iOS 27.2.
extension Date {
    @_silgen_name("$s10Foundation4DateV2leoiySbAC_ACtFZ")
    public static func _lessOrEqual(_ lhs: Date, _ rhs: Date) -> Bool {
        lhs.timeIntervalSinceReferenceDate <= rhs.timeIntervalSinceReferenceDate
    }

    @_silgen_name("$s10Foundation4DateV2geoiySbAC_ACtFZ")
    public static func _greaterOrEqual(_ lhs: Date, _ rhs: Date) -> Bool {
        lhs.timeIntervalSinceReferenceDate >= rhs.timeIntervalSinceReferenceDate
    }
}

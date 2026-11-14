import SwiftUI
import CodyncKit
import CodyncUI

struct LanguageSettings: View {
    @AppStorage(AppLanguage.storageKey, store: AppLanguage.defaults) private var storedLanguage = AppLanguage.system.rawValue

    private var selection: Binding<AppLanguage> {
        Binding(get: { AppLanguage(rawValue: storedLanguage) ?? .system }, set: { AppLanguage.setPreference($0) })
    }

    var body: some View {
        CardSection("Language", footer: "Choose the language for this device. Unsupported system languages use English.") {
            ValueRow("App language") {
                ChoicePicker(selection: selection, options: AppLanguage.allCases.map {
                    ($0, AppLanguage.menuTitle($0, preference: selection.wrappedValue, systemIsChinese: AppLanguage.systemIsChinese()))
                })
            }
        }
    }
}

//
//  Kael_LauncherApp.swift
//  Kael Launcher
//

import SwiftUI

@main
struct Kael_LauncherApp: App {
    @StateObject private var accountManager = AccountManager.shared

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(accountManager)
        }
        .windowStyle(.hiddenTitleBar)
        .defaultSize(
            width: WindowConfiguration.defaultSize.width,
            height: WindowConfiguration.defaultSize.height
        )
        .commands {
            CommandMenu("Account") {
            }
        }

        Settings {
            SettingsView()
        }
    }
}

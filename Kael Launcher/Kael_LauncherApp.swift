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
                if let active = accountManager.activeAccount {
                    ForEach(accountManager.accounts) { account in
                        Button(account.profile.name) {
                            Task { await accountManager.switchAccount(to: account.id) }
                        }
                        .disabled(account.id == active.id)
                    }
                    Divider()
                    Button("Add Account…") {
                        Task { await accountManager.login() }
                    }
                    Button("Sign Out of \(active.profile.name)") {
                        Task { await accountManager.signOut(active.id) }
                    }
                } else {
                    Button("Sign In…") {
                        Task { await accountManager.login() }
                    }
                }
            }
        }

        Settings {
            SettingsView()
        }
    }
}

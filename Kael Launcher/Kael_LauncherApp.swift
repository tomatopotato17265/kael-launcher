//
//  Kael_LauncherApp.swift
//  Kael Launcher
//

import SwiftUI

@main
struct Kael_LauncherApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
        }
        .windowStyle(.hiddenTitleBar)
        .defaultSize(
            width: WindowConfiguration.defaultSize.width,
            height: WindowConfiguration.defaultSize.height
        )
    }
}

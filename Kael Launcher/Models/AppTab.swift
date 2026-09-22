//
//  AppTab.swift
//  Kael Launcher
//

import Foundation

enum AppTab: String, CaseIterable, Identifiable {
    case home = "Home"
    case discover = "Discover"
    case skins = "Skins"
    case library = "Library"
    case search = "Search"

    var id: String { rawValue }
}

//
//  Instance.swift
//  Kael Launcher
//

import Foundation

nonisolated enum ModLoader: String, Codable {
    case vanilla
    case fabric
    case quilt
    case forge
    case neoforge
}

nonisolated enum InstanceInstallStage: String, Codable {
    case notInstalled
    case installing
    case installed
}

nonisolated struct Instance: Codable, Identifiable, Equatable {
    var id: UUID
    var name: String
    var iconPath: String?
    var minecraftVersion: String
    var loader: ModLoader
    var loaderVersion: String?
    var createdAt: Date
    var lastPlayedAt: Date?
    var installStage: InstanceInstallStage

    init(
        id: UUID = UUID(),
        name: String,
        iconPath: String? = nil,
        minecraftVersion: String,
        loader: ModLoader = .vanilla,
        loaderVersion: String? = nil,
        createdAt: Date = Date(),
        lastPlayedAt: Date? = nil,
        installStage: InstanceInstallStage = .notInstalled
    ) {
        self.id = id
        self.name = name
        self.iconPath = iconPath
        self.minecraftVersion = minecraftVersion
        self.loader = loader
        self.loaderVersion = loaderVersion
        self.createdAt = createdAt
        self.lastPlayedAt = lastPlayedAt
        self.installStage = installStage
    }
}

//
//  VersionManifest.swift
//  Kael Launcher
//

import Foundation

nonisolated enum MinecraftVersionType: String, Codable {
    case release
    case snapshot
    case oldBeta = "old_beta"
    case oldAlpha = "old_alpha"
}

nonisolated struct MinecraftVersionManifestLatest: Codable, Equatable {
    let release: String
    let snapshot: String
}

nonisolated struct MinecraftVersionSummary: Codable, Identifiable, Equatable {
    let id: String
    let type: MinecraftVersionType
    let url: URL
    let time: Date
    let releaseTime: Date
    let sha1: String
    let complianceLevel: Int
}

nonisolated struct MinecraftVersionManifest: Codable, Equatable {
    let latest: MinecraftVersionManifestLatest
    let versions: [MinecraftVersionSummary]

    var releases: [MinecraftVersionSummary] {
        versions.filter { $0.type == .release }
    }

    var snapshots: [MinecraftVersionSummary] {
        versions.filter { $0.type == .snapshot }
    }
}

//
//  VersionInfo.swift
//  Kael Launcher
//

import Foundation

nonisolated struct MinecraftDownload: Decodable, Equatable {
    let sha1: String
    let size: Int
    let url: URL
}

nonisolated struct MinecraftVersionDownloads: Decodable, Equatable {
    let client: MinecraftDownload
}

nonisolated struct MinecraftAssetIndexReference: Decodable, Equatable {
    let id: String
    let sha1: String
    let size: Int
    let totalSize: Int
    let url: URL
}

nonisolated struct MinecraftJavaVersion: Decodable, Equatable {
    let component: String
    let majorVersion: Int
}

nonisolated enum MinecraftRuleAction: String, Decodable {
    case allow
    case disallow
}

nonisolated struct MinecraftOsRule: Decodable, Equatable {
    let name: String?
    let version: String?
    let arch: String?
}

nonisolated struct MinecraftRule: Decodable, Equatable {
    let action: MinecraftRuleAction
    let os: MinecraftOsRule?
    let features: [String: Bool]?
}

nonisolated enum MinecraftArgument: Decodable, Equatable {
    case plain(String)
    case conditional(rules: [MinecraftRule], values: [String])

    private enum CodingKeys: String, CodingKey {
        case rules
        case value
    }

    init(from decoder: Decoder) throws {
        let single = try decoder.singleValueContainer()
        if let string = try? single.decode(String.self) {
            self = .plain(string)
            return
        }
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let rules = try container.decodeIfPresent([MinecraftRule].self, forKey: .rules) ?? []
        let values: [String]
        if let many = try? container.decode([String].self, forKey: .value) {
            values = many
        } else {
            values = [try container.decode(String.self, forKey: .value)]
        }
        self = .conditional(rules: rules, values: values)
    }
}

nonisolated struct MinecraftArguments: Decodable, Equatable {
    let game: [MinecraftArgument]
    let jvm: [MinecraftArgument]

    private enum CodingKeys: String, CodingKey {
        case game
        case jvm
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        game = try container.decodeIfPresent([MinecraftArgument].self, forKey: .game) ?? []
        jvm = try container.decodeIfPresent([MinecraftArgument].self, forKey: .jvm) ?? []
    }
}

nonisolated struct MinecraftLibraryArtifact: Decodable, Equatable {
    let path: String
    let sha1: String
    let size: Int
    let url: String
}

nonisolated struct MinecraftLibraryDownloads: Decodable, Equatable {
    let artifact: MinecraftLibraryArtifact?
    let classifiers: [String: MinecraftLibraryArtifact]?
}

nonisolated struct MinecraftLibrary: Decodable, Equatable {
    let name: String
    let url: String?
    let downloads: MinecraftLibraryDownloads?
    let natives: [String: String]?
    let rules: [MinecraftRule]?
}

nonisolated struct MinecraftVersionInfo: Decodable, Equatable {
    let id: String
    let type: MinecraftVersionType
    let mainClass: String
    let minecraftArguments: String?
    let arguments: MinecraftArguments?
    let assetIndex: MinecraftAssetIndexReference
    let assets: String
    let downloads: MinecraftVersionDownloads
    let javaVersion: MinecraftJavaVersion?
    let libraries: [MinecraftLibrary]
    let releaseTime: Date
    let time: Date

    var requiredJavaMajorVersion: Int {
        javaVersion?.majorVersion ?? 8
    }
}

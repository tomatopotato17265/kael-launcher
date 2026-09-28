//
//  LauncherDirectories.swift
//  Kael Launcher
//

import Foundation

nonisolated enum LauncherDirectories {
    static var applicationSupportDirectory: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return base.appendingPathComponent("Kael Launcher", isDirectory: true)
    }

    static var metaDirectory: URL {
        directory("meta")
    }

    static var versionsDirectory: URL {
        directory("versions")
    }

    static var librariesDirectory: URL {
        directory("libraries")
    }

    static var assetsDirectory: URL {
        directory("assets")
    }

    static var assetsIndexDirectory: URL {
        directory("assets/indexes")
    }

    static var objectsDirectory: URL {
        directory("assets/objects")
    }

    static var javaDirectory: URL {
        directory("java")
    }

    static var instancesDirectory: URL {
        directory("instances")
    }

    static var iconsDirectory: URL {
        directory("icons")
    }

    static func versionDirectory(_ version: String) -> URL {
        directory("versions/\(version)")
    }

    static func versionNativesDirectory(_ version: String) -> URL {
        directory("versions/\(version)/natives")
    }

    static func javaVersionDirectory(major: Int) -> URL {
        directory("java/\(major)")
    }

    static func objectDirectory(hash: String) -> URL {
        let prefix = String(hash.prefix(2))
        return directory("assets/objects/\(prefix)")
    }

    static func instanceDirectory(id: UUID) -> URL {
        directory("instances/\(id.uuidString)")
    }

    static func instanceLogsDirectory(id: UUID) -> URL {
        directory("instances/\(id.uuidString)/logs")
    }

    private static func directory(_ relativePath: String) -> URL {
        let url = applicationSupportDirectory.appendingPathComponent(relativePath, isDirectory: true)
        try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }
}

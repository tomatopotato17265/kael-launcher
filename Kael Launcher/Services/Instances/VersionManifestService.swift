//
//  VersionManifestService.swift
//  Kael Launcher
//

import Foundation

actor VersionManifestService {
    static let shared = VersionManifestService()

    private static let manifestURL = URL(string: "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")!
    private static let cacheTTL: TimeInterval = 60 * 60 * 24

    private let session: URLSession
    private let decoder: JSONDecoder
    private var cachedManifest: MinecraftVersionManifest?

    init(session: URLSession = .shared) {
        self.session = session
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        self.decoder = decoder
    }

    private var cacheFileURL: URL {
        LauncherDirectories.metaDirectory.appendingPathComponent("version_manifest_v2.json")
    }

    func fetchManifest(forceRefresh: Bool = false) async throws -> MinecraftVersionManifest {
        if let cachedManifest, !forceRefresh {
            return cachedManifest
        }

        if !forceRefresh, let onDisk = try? loadCachedManifest(), isFresh(at: cacheFileURL) {
            cachedManifest = onDisk
            return onDisk
        }

        do {
            let manifest = try await downloadManifest()
            cachedManifest = manifest
            try? persist(manifest)
            return manifest
        } catch {
            if let onDisk = try? loadCachedManifest() {
                cachedManifest = onDisk
                return onDisk
            }
            throw error
        }
    }

    private func downloadManifest() async throws -> MinecraftVersionManifest {
        let (data, response) = try await session.data(from: Self.manifestURL)
        guard let httpResponse = response as? HTTPURLResponse, (200..<300).contains(httpResponse.statusCode) else {
            throw VersionManifestError.invalidResponse
        }
        return try decoder.decode(MinecraftVersionManifest.self, from: data)
    }

    private func loadCachedManifest() throws -> MinecraftVersionManifest {
        let data = try Data(contentsOf: cacheFileURL)
        return try decoder.decode(MinecraftVersionManifest.self, from: data)
    }

    private func persist(_ manifest: MinecraftVersionManifest) throws {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        let data = try encoder.encode(manifest)
        try data.write(to: cacheFileURL, options: .atomic)
    }

    private func isFresh(at url: URL) -> Bool {
        guard let attributes = try? FileManager.default.attributesOfItem(atPath: url.path),
              let modificationDate = attributes[.modificationDate] as? Date else {
            return false
        }
        return Date().timeIntervalSince(modificationDate) < Self.cacheTTL
    }
}

nonisolated enum VersionManifestError: Error {
    case invalidResponse
}

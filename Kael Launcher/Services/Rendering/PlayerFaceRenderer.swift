//
//  PlayerFaceRenderer.swift
//  Kael Launcher
//

import AppKit
import Foundation
import ImageIO

nonisolated enum PlayerFaceRendererError: Error, LocalizedError {
    case invalidImageData
    case renderingFailed

    var errorDescription: String? {
        switch self {
        case .invalidImageData:
            return "The skin texture could not be loaded."
        case .renderingFailed:
            return "Failed to render the player's face."
        }
    }
}

actor PlayerFaceRenderer {
    static let shared = PlayerFaceRenderer()

    private var cache: [String: NSImage] = [:]
    private let session: URLSession

    init(session: URLSession = .shared) {
        self.session = session
    }

    func face(for skin: MinecraftSkin, size: CGFloat = 64) async throws -> NSImage {
        let cacheKey = "\(skin.resolvedTextureKey)-\(Int(size))"
        if let cached = cache[cacheKey] {
            return cached
        }

        let (data, _) = try await session.data(from: Self.secureURL(for: skin.url))
        guard let cgImage = Self.decodeCGImage(from: data) else {
            throw PlayerFaceRendererError.invalidImageData
        }

        let rendered = try Self.renderFace(from: cgImage, size: size)
        cache[cacheKey] = rendered
        return rendered
    }

    private static func secureURL(for url: URL) -> URL {
        guard url.scheme == "http",
              var components = URLComponents(url: url, resolvingAgainstBaseURL: false) else {
            return url
        }
        components.scheme = "https"
        return components.url ?? url
    }

    private static func decodeCGImage(from data: Data) -> CGImage? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil) else {
            return nil
        }
        return CGImageSourceCreateImageAtIndex(source, 0, nil)
    }

    private static func renderFace(from source: CGImage, size: CGFloat) throws -> NSImage {
        guard let base = source.cropping(to: CGRect(x: 8, y: 8, width: 8, height: 8)) else {
            throw PlayerFaceRendererError.invalidImageData
        }
        let hat = source.cropping(to: CGRect(x: 40, y: 8, width: 8, height: 8))

        let outputSize = CGSize(width: size, height: size)
        guard let context = CGContext(
            data: nil,
            width: Int(size),
            height: Int(size),
            bitsPerComponent: 8,
            bytesPerRow: 0,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ) else {
            throw PlayerFaceRendererError.renderingFailed
        }

        context.interpolationQuality = .none
        context.draw(base, in: CGRect(origin: .zero, size: outputSize))

        if let hat, hasVisiblePixels(hat) {
            context.draw(hat, in: CGRect(origin: .zero, size: outputSize))
        }

        guard let outputImage = context.makeImage() else {
            throw PlayerFaceRendererError.renderingFailed
        }

        return NSImage(cgImage: outputImage, size: outputSize)
    }

    private static func hasVisiblePixels(_ image: CGImage) -> Bool {
        let width = image.width
        let height = image.height

        guard let context = CGContext(
            data: nil,
            width: width,
            height: height,
            bitsPerComponent: 8,
            bytesPerRow: 0,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ) else {
            return false
        }

        context.interpolationQuality = .none
        context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))

        guard let data = context.data else {
            return false
        }

        let bytesPerRow = context.bytesPerRow
        let pointer = data.bindMemory(to: UInt8.self, capacity: bytesPerRow * height)

        for y in 0..<height {
            let rowStart = y * bytesPerRow
            for x in 0..<width {
                let alphaIndex = rowStart + x * 4 + 3
                if pointer[alphaIndex] > 0 {
                    return true
                }
            }
        }
        return false
    }
}

//
//  XboxSignedRequest.swift
//  Kael Launcher
//

import CryptoKit
import Foundation

extension Data {
    func base64URLEncodedString() -> String {
        base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }
}

private extension FixedWidthInteger {
    var bigEndianBytes: [UInt8] {
        withUnsafeBytes(of: bigEndian, Array.init)
    }
}

struct XboxDeviceTokenKey {
    let id: UUID
    let privateKey: P256.Signing.PrivateKey
    let x: String
    let y: String

    static func generate() -> XboxDeviceTokenKey {
        let privateKey = P256.Signing.PrivateKey()
        let coordinates = privateKey.publicKey.x963Representation.dropFirst()
        let x = coordinates.prefix(32)
        let y = coordinates.suffix(32)
        return XboxDeviceTokenKey(
            id: UUID(),
            privateKey: privateKey,
            x: Data(x).base64URLEncodedString(),
            y: Data(y).base64URLEncodedString()
        )
    }

    var braceUppercasedId: String {
        "{\(id.uuidString.uppercased())}"
    }

    var proofKeyJSON: [String: Any] {
        [
            "kty": "EC",
            "x": x,
            "y": y,
            "crv": "P-256",
            "alg": "ES256",
            "use": "sig",
        ]
    }
}

struct XboxSignedResponse<T> {
    let value: T
    let response: HTTPURLResponse
    let currentDate: Date
}

enum XboxSignedRequest {
    static let userAgent = "Kael Launcher (https://github.com/tomatopotato17265/kael-launcher)"

    static let jsonDecoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .custom { decoder in
            let container = try decoder.singleValueContainer()
            let string = try container.decode(String.self)
            guard let date = parseXboxDate(string) else {
                throw DecodingError.dataCorruptedError(
                    in: container,
                    debugDescription: "Unrecognized date format: \(string)"
                )
            }
            return date
        }
        return decoder
    }()

    static func parseXboxDate(_ string: String) -> Date? {
        var trimmed = string
        if let dotIndex = trimmed.firstIndex(of: "."),
           let zIndex = trimmed.firstIndex(of: "Z") {
            let fractionStart = trimmed.index(after: dotIndex)
            let fraction = trimmed[fractionStart..<zIndex].prefix(3)
            trimmed = String(trimmed[..<fractionStart]) + fraction + "Z"
        }

        let withFraction = ISO8601DateFormatter()
        withFraction.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let date = withFraction.date(from: trimmed) {
            return date
        }

        let withoutFraction = ISO8601DateFormatter()
        withoutFraction.formatOptions = [.withInternetDateTime]
        return withoutFraction.date(from: trimmed)
    }

    static func date(from response: HTTPURLResponse) -> Date {
        guard let value = response.value(forHTTPHeaderField: "Date") else {
            return Date()
        }
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(identifier: "GMT")
        formatter.dateFormat = "EEE, dd MMM yyyy HH:mm:ss zzz"
        return formatter.date(from: value) ?? Date()
    }

    static func performRequest(
        _ request: URLRequest,
        session: URLSession = .shared
    ) async throws -> (Data, HTTPURLResponse) {
        let maxAttempts = 5
        var lastError: Error = MinecraftAuthError.invalidResponse(step: request.url?.absoluteString ?? "unknown")

        for attempt in 0..<maxAttempts {
            do {
                let (data, response) = try await session.data(for: request)
                guard let httpResponse = response as? HTTPURLResponse else {
                    throw MinecraftAuthError.invalidResponse(step: request.url?.absoluteString ?? "unknown")
                }
                return (data, httpResponse)
            } catch let error as URLError
                where error.code == .timedOut
                    || error.code == .cannotConnectToHost
                    || error.code == .networkConnectionLost
                    || error.code == .notConnectedToInternet {
                lastError = error
                if attempt < maxAttempts - 1 {
                    try? await Task.sleep(nanoseconds: 250_000_000)
                }
            }
        }

        throw lastError
    }

    static func send<T: Decodable>(
        to url: URL,
        urlPath: String,
        body: [String: Any],
        authorization: String? = nil,
        key: XboxDeviceTokenKey,
        currentDate: Date,
        includeContractVersionHeader: Bool = true,
        session: URLSession = .shared
    ) async throws -> XboxSignedResponse<T> {
        let bodyData = try JSONSerialization.data(withJSONObject: body)
        let signature = try signature(
            for: bodyData,
            urlPath: urlPath,
            authorization: authorization,
            key: key,
            currentDate: currentDate
        )

        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.httpBody = bodyData
        request.setValue("application/json; charset=utf-8", forHTTPHeaderField: "Content-Type")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue(signature, forHTTPHeaderField: "Signature")
        if includeContractVersionHeader {
            request.setValue("1", forHTTPHeaderField: "x-xbl-contract-version")
        }
        if let authorization {
            request.setValue(authorization, forHTTPHeaderField: "Authorization")
        }

        let (data, response) = try await performRequest(request, session: session)

        guard (200..<300).contains(response.statusCode) else {
            throw MinecraftAuthError.invalidResponse(step: "\(urlPath) (status \(response.statusCode))")
        }

        let value: T
        do {
            value = try jsonDecoder.decode(T.self, from: data)
        } catch {
            throw MinecraftAuthError.requestFailed(step: urlPath, underlying: error)
        }

        return XboxSignedResponse(value: value, response: response, currentDate: date(from: response))
    }

    private static func signature(
        for body: Data,
        urlPath: String,
        authorization: String?,
        key: XboxDeviceTokenKey,
        currentDate: Date
    ) throws -> String {
        let unixTimestamp = UInt64(max(currentDate.timeIntervalSince1970, 0))
        let windowsFileTime = (unixTimestamp &+ 11_644_473_600) &* 10_000_000

        var buffer = Data()
        buffer.append(contentsOf: UInt32(1).bigEndianBytes)
        buffer.append(0)
        buffer.append(contentsOf: windowsFileTime.bigEndianBytes)
        buffer.append(0)
        buffer.append(contentsOf: Array("POST".utf8))
        buffer.append(0)
        buffer.append(contentsOf: Array(urlPath.utf8))
        buffer.append(0)
        if let authorization, let authData = authorization.data(using: .utf8) {
            buffer.append(authData)
        }
        buffer.append(0)
        buffer.append(body)
        buffer.append(0)

        let ecdsaSignature = try key.privateKey.signature(for: buffer)

        var signatureBuffer = Data()
        signatureBuffer.append(contentsOf: Int32(1).bigEndianBytes)
        signatureBuffer.append(contentsOf: windowsFileTime.bigEndianBytes)
        signatureBuffer.append(ecdsaSignature.rawRepresentation)

        return signatureBuffer.base64EncodedString()
    }
}

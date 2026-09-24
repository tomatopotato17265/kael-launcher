//
//  MinecraftAuthService.swift
//  Kael Launcher
//

import CryptoKit
import Foundation
import Security

actor MinecraftAuthService {
    static let shared = MinecraftAuthService()

    private static let microsoftClientId = "00000000402b5328"
    private static let authReplyURL = URL(string: "https://login.live.com/oauth20_desktop.srf")!
    private static let requestedScope = "service::user.auth.xboxlive.com::MBI_SSL"

    private var deviceTokenPair: (key: XboxDeviceTokenKey, token: XboxDeviceToken)?

    func beginLogin() async throws -> MinecraftLoginFlow {
        let (key, token, currentDate) = try await refreshedDeviceTokenPair(currentDate: Date())

        let verifier = Self.generatePKCEVerifier()
        let challenge = Self.codeChallenge(for: verifier)

        let (sessionId, response) = try await sisuAuthenticate(
            deviceToken: token.token,
            challenge: challenge,
            key: key,
            currentDate: currentDate
        )

        guard let authRequestURL = URL(string: response.value.msaOauthRedirect) else {
            throw MinecraftAuthError.invalidResponse(step: "sisuAuthenticate redirect")
        }

        return MinecraftLoginFlow(
            verifier: verifier,
            challenge: challenge,
            sessionId: sessionId,
            authRequestURI: authRequestURL
        )
    }

    // MARK: - Device token

    private func refreshedDeviceTokenPair(
        currentDate: Date
    ) async throws -> (key: XboxDeviceTokenKey, token: XboxDeviceToken, date: Date) {
        if let pair = deviceTokenPair, pair.token.notAfter > currentDate {
            return (pair.key, pair.token, currentDate)
        }

        let key = deviceTokenPair?.key ?? XboxDeviceTokenKey.generate()
        let response = try await requestDeviceToken(key: key, currentDate: currentDate)
        deviceTokenPair = (key, response.value)
        return (key, response.value, response.currentDate)
    }

    private func requestDeviceToken(
        key: XboxDeviceTokenKey,
        currentDate: Date
    ) async throws -> XboxSignedResponse<XboxDeviceToken> {
        let body: [String: Any] = [
            "Properties": [
                "AuthMethod": "ProofOfPossession",
                "Id": key.braceUppercasedId,
                "DeviceType": "Win32",
                "Version": "10.16.0",
                "ProofKey": key.proofKeyJSON,
            ],
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT",
        ]

        return try await XboxSignedRequest.send(
            to: URL(string: "https://device.auth.xboxlive.com/device/authenticate")!,
            urlPath: "/device/authenticate",
            body: body,
            key: key,
            currentDate: currentDate
        )
    }

    // MARK: - SISU authenticate

    private func sisuAuthenticate(
        deviceToken: String,
        challenge: String,
        key: XboxDeviceTokenKey,
        currentDate: Date
    ) async throws -> (sessionId: String, response: XboxSignedResponse<XboxSisuAuthenticateResponse>) {
        let body: [String: Any] = [
            "AppId": Self.microsoftClientId,
            "DeviceToken": deviceToken,
            "Offers": [Self.requestedScope],
            "Query": [
                "code_challenge": challenge,
                "code_challenge_method": "S256",
                "state": Self.generatePKCEVerifier(),
                "prompt": "select_account",
            ],
            "RedirectUri": Self.authReplyURL.absoluteString,
            "Sandbox": "RETAIL",
            "TokenType": "code",
            "TitleId": "1794566092",
        ]

        let response: XboxSignedResponse<XboxSisuAuthenticateResponse> = try await XboxSignedRequest.send(
            to: URL(string: "https://sisu.xboxlive.com/authenticate")!,
            urlPath: "/authenticate",
            body: body,
            key: key,
            currentDate: currentDate
        )

        guard let sessionId = response.response.value(forHTTPHeaderField: "X-SessionId") else {
            throw MinecraftAuthError.missingSessionId
        }

        return (sessionId, response)
    }

    // MARK: - PKCE helpers

    private static func generatePKCEVerifier() -> String {
        var bytes = [UInt8](repeating: 0, count: 64)
        _ = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
        return bytes.map { String(format: "%02x", $0) }.joined()
    }

    private static func codeChallenge(for verifier: String) -> String {
        let digest = SHA256.hash(data: Data(verifier.utf8))
        return Data(digest).base64URLEncodedString()
    }
}

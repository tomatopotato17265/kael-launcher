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

    func finishLogin(code: String, flow: MinecraftLoginFlow) async throws -> MinecraftCredentials {
        let (key, token, _) = try await refreshedDeviceTokenPair(currentDate: Date())

        let oauthToken = try await requestOAuthToken(code: code, verifier: flow.verifier)

        let sisuAuthorizeResponse = try await sisuAuthorize(
            sessionId: flow.sessionId,
            accessToken: oauthToken.value.accessToken,
            deviceToken: token.token,
            key: key,
            currentDate: oauthToken.date
        )

        let xboxToken = try await xstsAuthorize(
            authorize: sisuAuthorizeResponse.value,
            deviceToken: token.token,
            key: key,
            currentDate: sisuAuthorizeResponse.currentDate
        )

        let minecraftToken = try await requestMinecraftToken(xboxToken: xboxToken.value)

        try await checkEntitlements(accessToken: minecraftToken.accessToken)

        let profile = try await fetchProfile(accessToken: minecraftToken.accessToken)

        let credentials = MinecraftCredentials(
            profile: profile,
            accessToken: minecraftToken.accessToken,
            refreshToken: oauthToken.value.refreshToken,
            expires: oauthToken.date.addingTimeInterval(TimeInterval(oauthToken.value.expiresIn)),
            active: true
        )

        try await CredentialStore.shared.upsert(credentials)
        try await CredentialStore.shared.setActive(id: credentials.id)

        return credentials
    }

    func refresh(_ credentials: MinecraftCredentials) async throws -> MinecraftCredentials {
        guard credentials.isExpired else {
            return credentials
        }

        let oauthToken = try await requestOAuthToken(refreshToken: credentials.refreshToken)
        let (key, token, _) = try await refreshedDeviceTokenPair(currentDate: oauthToken.date)

        let sisuAuthorizeResponse = try await sisuAuthorize(
            sessionId: nil,
            accessToken: oauthToken.value.accessToken,
            deviceToken: token.token,
            key: key,
            currentDate: oauthToken.date
        )

        let xboxToken = try await xstsAuthorize(
            authorize: sisuAuthorizeResponse.value,
            deviceToken: token.token,
            key: key,
            currentDate: sisuAuthorizeResponse.currentDate
        )

        let minecraftToken = try await requestMinecraftToken(xboxToken: xboxToken.value)

        var updated = credentials
        updated.accessToken = minecraftToken.accessToken
        updated.refreshToken = oauthToken.value.refreshToken
        updated.expires = oauthToken.date.addingTimeInterval(TimeInterval(oauthToken.value.expiresIn))

        try await CredentialStore.shared.upsert(updated)

        return updated
    }

    private func refreshedDeviceTokenPair(
        currentDate: Date
    ) async throws -> (key: XboxDeviceTokenKey, token: XboxDeviceToken, date: Date) {
        if let pair = deviceTokenPair, pair.token.notAfter > currentDate {
            return (pair.key, pair.token, currentDate)
        }

        if deviceTokenPair == nil, let stored = try? await CredentialStore.shared.deviceTokenPair() {
            deviceTokenPair = stored
            if stored.token.notAfter > currentDate {
                return (stored.key, stored.token, currentDate)
            }
        }

        let key = deviceTokenPair?.key ?? XboxDeviceTokenKey.generate()
        let response = try await requestDeviceToken(key: key, currentDate: currentDate)
        deviceTokenPair = (key, response.value)
        try? await CredentialStore.shared.setDeviceTokenPair(key: key, token: response.value)
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

    private static func generatePKCEVerifier() -> String {
        var bytes = [UInt8](repeating: 0, count: 64)
        _ = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
        return bytes.map { String(format: "%02x", $0) }.joined()
    }

    private static func codeChallenge(for verifier: String) -> String {
        let digest = SHA256.hash(data: Data(verifier.utf8))
        return Data(digest).base64URLEncodedString()
    }

    private func requestOAuthToken(
        code: String,
        verifier: String
    ) async throws -> (value: MicrosoftOAuthTokenResponse, date: Date) {
        try await requestOAuthToken(parameters: [
            "client_id": Self.microsoftClientId,
            "code": code,
            "code_verifier": verifier,
            "grant_type": "authorization_code",
            "redirect_uri": Self.authReplyURL.absoluteString,
            "scope": Self.requestedScope,
        ])
    }

    private func requestOAuthToken(
        refreshToken: String
    ) async throws -> (value: MicrosoftOAuthTokenResponse, date: Date) {
        try await requestOAuthToken(parameters: [
            "client_id": Self.microsoftClientId,
            "refresh_token": refreshToken,
            "grant_type": "refresh_token",
            "redirect_uri": Self.authReplyURL.absoluteString,
            "scope": Self.requestedScope,
        ])
    }

    private func requestOAuthToken(
        parameters: [String: String]
    ) async throws -> (value: MicrosoftOAuthTokenResponse, date: Date) {
        var request = URLRequest(url: URL(string: "https://login.live.com/oauth20_token.srf")!)
        request.httpMethod = "POST"
        request.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.httpBody = Self.formEncode(parameters).data(using: .utf8)

        let (data, response) = try await XboxSignedRequest.performRequest(request)

        guard (200..<300).contains(response.statusCode) else {
            if let errorResponse = try? XboxSignedRequest.jsonDecoder.decode(MicrosoftOAuthErrorResponse.self, from: data) {
                throw MinecraftAuthError.oauthError(errorResponse.error)
            }
            throw MinecraftAuthError.invalidResponse(step: "oauth token (status \(response.statusCode))")
        }

        let value = try XboxSignedRequest.jsonDecoder.decode(MicrosoftOAuthTokenResponse.self, from: data)
        return (value, XboxSignedRequest.date(from: response))
    }

    private static func formEncode(_ parameters: [String: String]) -> String {
        let allowed = CharacterSet.urlQueryAllowed.subtracting(CharacterSet(charactersIn: "+&="))
        return parameters.map { key, value in
            let encodedKey = key.addingPercentEncoding(withAllowedCharacters: allowed) ?? key
            let encodedValue = value.addingPercentEncoding(withAllowedCharacters: allowed) ?? value
            return "\(encodedKey)=\(encodedValue)"
        }.joined(separator: "&")
    }

    private func sisuAuthorize(
        sessionId: String?,
        accessToken: String,
        deviceToken: String,
        key: XboxDeviceTokenKey,
        currentDate: Date
    ) async throws -> XboxSignedResponse<XboxSisuAuthorizeResponse> {
        let body: [String: Any] = [
            "AccessToken": "t=\(accessToken)",
            "AppId": Self.microsoftClientId,
            "DeviceToken": deviceToken,
            "ProofKey": key.proofKeyJSON,
            "Sandbox": "RETAIL",
            "SessionId": sessionId ?? NSNull(),
            "SiteName": "user.auth.xboxlive.com",
            "RelyingParty": "http://xboxlive.com",
            "UseModernGamertag": true,
        ]

        return try await XboxSignedRequest.send(
            to: URL(string: "https://sisu.xboxlive.com/authorize")!,
            urlPath: "/authorize",
            body: body,
            key: key,
            currentDate: currentDate,
            includeContractVersionHeader: false
        )
    }

    private func xstsAuthorize(
        authorize: XboxSisuAuthorizeResponse,
        deviceToken: String,
        key: XboxDeviceTokenKey,
        currentDate: Date
    ) async throws -> XboxSignedResponse<XboxDeviceToken> {
        let body: [String: Any] = [
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT",
            "Properties": [
                "SandboxId": "RETAIL",
                "UserTokens": [authorize.userToken.token],
                "DeviceToken": deviceToken,
                "TitleToken": authorize.titleToken.token,
            ],
        ]

        return try await XboxSignedRequest.send(
            to: URL(string: "https://xsts.auth.xboxlive.com/xsts/authorize")!,
            urlPath: "/xsts/authorize",
            body: body,
            key: key,
            currentDate: currentDate
        )
    }

    private func requestMinecraftToken(xboxToken: XboxDeviceToken) async throws -> MinecraftLauncherLoginResponse {
        guard let uhs = xboxToken.displayClaims?.xui.first?.uhs else {
            throw MinecraftAuthError.missingUserHash
        }

        var request = URLRequest(url: URL(string: "https://api.minecraftservices.com/launcher/login")!)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue(XboxSignedRequest.userAgent, forHTTPHeaderField: "User-Agent")
        request.httpBody = try JSONSerialization.data(withJSONObject: [
            "platform": "PC_LAUNCHER",
            "xtoken": "XBL3.0 x=\(uhs);\(xboxToken.token)",
        ])

        let (data, response) = try await XboxSignedRequest.performRequest(request)

        guard (200..<300).contains(response.statusCode) else {
            throw MinecraftAuthError.invalidResponse(step: "minecraft launcher login (status \(response.statusCode))")
        }

        return try XboxSignedRequest.jsonDecoder.decode(MinecraftLauncherLoginResponse.self, from: data)
    }

    private func checkEntitlements(accessToken: String) async throws {
        var request = URLRequest(
            url: URL(string: "https://api.minecraftservices.com/entitlements/license?requestId=\(UUID().uuidString)")!
        )
        request.httpMethod = "GET"
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue(XboxSignedRequest.userAgent, forHTTPHeaderField: "User-Agent")
        request.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")

        let (_, response) = try await XboxSignedRequest.performRequest(request)

        guard (200..<300).contains(response.statusCode) else {
            throw MinecraftAuthError.notEntitled
        }
    }

    private func fetchProfile(accessToken: String) async throws -> MinecraftProfile {
        var request = URLRequest(url: URL(string: "https://api.minecraftservices.com/minecraft/profile")!)
        request.httpMethod = "GET"
        request.timeoutInterval = 10
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue(XboxSignedRequest.userAgent, forHTTPHeaderField: "User-Agent")
        request.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")

        let (data, response) = try await XboxSignedRequest.performRequest(request)

        guard (200..<300).contains(response.statusCode) else {
            throw MinecraftAuthError.invalidResponse(step: "minecraft profile (status \(response.statusCode))")
        }

        return try XboxSignedRequest.jsonDecoder.decode(MinecraftProfile.self, from: data)
    }
}

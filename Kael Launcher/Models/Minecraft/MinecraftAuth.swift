//
//  MinecraftAuth.swift
//  Kael Launcher
//

import Foundation

nonisolated extension UUID {
    init?(mojangString string: String) {
        if let uuid = UUID(uuidString: string) {
            self = uuid
            return
        }
        let chars = Array(string)
        guard chars.count == 32, chars.allSatisfy(\.isHexDigit) else {
            return nil
        }
        let hyphenated = [
            String(chars[0..<8]),
            String(chars[8..<12]),
            String(chars[12..<16]),
            String(chars[16..<20]),
            String(chars[20..<32]),
        ].joined(separator: "-")
        guard let uuid = UUID(uuidString: hyphenated) else {
            return nil
        }
        self = uuid
    }
}

// the skin and cape
nonisolated enum MinecraftCharacterExpressionState: String, Codable {
    case active = "ACTIVE"
    case inactive = "INACTIVE"
    case unknown

    init(from decoder: Decoder) throws {
        let raw = try decoder.singleValueContainer().decode(String.self)
        self = MinecraftCharacterExpressionState(rawValue: raw) ?? .unknown
    }
}

// slim or classic skin
nonisolated enum MinecraftSkinVariant: String, Codable {
    case classic = "CLASSIC"
    case slim = "SLIM"
    case unknown

    init(from decoder: Decoder) throws {
        let raw = try decoder.singleValueContainer().decode(String.self)
        self = MinecraftSkinVariant(rawValue: raw) ?? .unknown
    }
}

nonisolated struct MinecraftSkin: Codable, Identifiable, Equatable {
    let id: UUID
    let state: MinecraftCharacterExpressionState
    let url: URL
    let textureKey: String?
    let variant: MinecraftSkinVariant
    let name: String?

    enum CodingKeys: String, CodingKey {
        case id
        case state
        case url
        case textureKey
        case variant
        case name = "alias"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let idString = try container.decode(String.self, forKey: .id)
        guard let id = UUID(mojangString: idString) else {
            throw DecodingError.dataCorruptedError(forKey: .id, in: container, debugDescription: "Invalid UUID: \(idString)")
        }
        self.id = id
        state = try container.decode(MinecraftCharacterExpressionState.self, forKey: .state)
        url = try container.decode(URL.self, forKey: .url)
        textureKey = try container.decodeIfPresent(String.self, forKey: .textureKey)
        variant = try container.decode(MinecraftSkinVariant.self, forKey: .variant)
        name = try container.decodeIfPresent(String.self, forKey: .name)
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encode(state, forKey: .state)
        try container.encode(url, forKey: .url)
        try container.encodeIfPresent(textureKey, forKey: .textureKey)
        try container.encode(variant, forKey: .variant)
        try container.encodeIfPresent(name, forKey: .name)
    }

    var resolvedTextureKey: String {
        textureKey ?? url.lastPathComponent
    }
}

nonisolated struct MinecraftCape: Codable, Identifiable, Equatable {
    let id: UUID
    let state: MinecraftCharacterExpressionState
    let url: URL
    let name: String

    enum CodingKeys: String, CodingKey {
        case id
        case state
        case url
        case name = "alias"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let idString = try container.decode(String.self, forKey: .id)
        guard let id = UUID(mojangString: idString) else {
            throw DecodingError.dataCorruptedError(forKey: .id, in: container, debugDescription: "Invalid UUID: \(idString)")
        }
        self.id = id
        state = try container.decode(MinecraftCharacterExpressionState.self, forKey: .state)
        url = try container.decode(URL.self, forKey: .url)
        name = try container.decode(String.self, forKey: .name)
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encode(state, forKey: .state)
        try container.encode(url, forKey: .url)
        try container.encode(name, forKey: .name)
    }
}

nonisolated struct MinecraftProfile: Codable, Equatable {
    var id: UUID
    var name: String
    var skins: [MinecraftSkin]
    var capes: [MinecraftCape]

    enum CodingKeys: String, CodingKey {
        case id
        case name
        case skins
        case capes
    }

    init(id: UUID, name: String, skins: [MinecraftSkin], capes: [MinecraftCape]) {
        self.id = id
        self.name = name
        self.skins = skins
        self.capes = capes
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let idString = try container.decode(String.self, forKey: .id)
        guard let id = UUID(mojangString: idString) else {
            throw DecodingError.dataCorruptedError(forKey: .id, in: container, debugDescription: "Invalid UUID: \(idString)")
        }
        self.id = id
        name = try container.decode(String.self, forKey: .name)
        skins = try container.decode([MinecraftSkin].self, forKey: .skins)
        capes = try container.decode([MinecraftCape].self, forKey: .capes)
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encode(name, forKey: .name)
        try container.encode(skins, forKey: .skins)
        try container.encode(capes, forKey: .capes)
    }

    static let empty = MinecraftProfile(id: UUID(), name: "", skins: [], capes: [])

    var currentSkin: MinecraftSkin? {
        skins.first { $0.state == .active }
    }

    var currentCape: MinecraftCape? {
        capes.first { $0.state == .active }
    }
}

nonisolated struct MinecraftCredentials: Codable, Identifiable, Equatable {
    var profile: MinecraftProfile
    var accessToken: String
    var refreshToken: String
    var expires: Date
    var active: Bool

    enum CodingKeys: String, CodingKey {
        case profile
        case accessToken = "access_token"
        case refreshToken = "refresh_token"
        case expires
        case active
    }

    var id: UUID { profile.id }

    var isExpired: Bool {
        expires <= Date().addingTimeInterval(5 * 60)
    }
}

nonisolated struct MinecraftLoginFlow {
    let verifier: String
    let challenge: String
    let sessionId: String
    let authRequestURI: URL
}

nonisolated enum MinecraftAuthError: Error, LocalizedError {
    case invalidResponse(step: String)
    case requestFailed(step: String, underlying: Error)
    case missingSessionId
    case missingUserHash
    case notEntitled
    case signInCancelled
    case oauthError(String)

    var errorDescription: String? {
        switch self {
        case .invalidResponse(let step):
            return "Received an unexpected response during \(step)."
        case .requestFailed(let step, let underlying):
            return "Request failed during \(step): \(underlying.localizedDescription)"
        case .missingSessionId:
            return "Xbox Live did not return a session ID."
        case .missingUserHash:
            return "Xbox Live did not return a user hash."
        case .notEntitled:
            return "This Microsoft account does not own Minecraft."
        case .signInCancelled:
            return "Sign-in was cancelled."
        case .oauthError(let error):
            return "Microsoft sign-in failed: \(error)"
        }
    }
}

nonisolated struct XboxDeviceToken: Codable {
    struct DisplayClaims: Codable {
        struct Xui: Codable {
            let uhs: String
        }
        let xui: [Xui]?
    }

    let issueInstant: Date
    let notAfter: Date
    let token: String
    let displayClaims: DisplayClaims?

    enum CodingKeys: String, CodingKey {
        case issueInstant = "IssueInstant"
        case notAfter = "NotAfter"
        case token = "Token"
        case displayClaims = "DisplayClaims"
    }
}

nonisolated struct XboxSisuAuthenticateResponse: Codable {
    let msaOauthRedirect: String

    enum CodingKeys: String, CodingKey {
        case msaOauthRedirect = "MsaOauthRedirect"
    }
}

nonisolated struct XboxSisuAuthorizeResponse: Codable {
    let titleToken: XboxDeviceToken
    let userToken: XboxDeviceToken

    enum CodingKeys: String, CodingKey {
        case titleToken = "TitleToken"
        case userToken = "UserToken"
    }
}

nonisolated struct MicrosoftOAuthTokenResponse: Codable {
    let expiresIn: Int
    let accessToken: String
    let refreshToken: String

    enum CodingKeys: String, CodingKey {
        case expiresIn = "expires_in"
        case accessToken = "access_token"
        case refreshToken = "refresh_token"
    }
}

nonisolated struct MicrosoftOAuthErrorResponse: Codable {
    let error: String
}

nonisolated struct MinecraftLauncherLoginResponse: Codable {
    let accessToken: String

    enum CodingKeys: String, CodingKey {
        case accessToken = "access_token"
    }
}

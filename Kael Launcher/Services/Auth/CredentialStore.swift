//
//  CredentialStore.swift
//  Kael Launcher
//

import CryptoKit
import Foundation
import Security

nonisolated enum CredentialStoreError: Error, LocalizedError {
    case keychain(OSStatus)

    var errorDescription: String? {
        switch self {
        case .keychain(let status):
            return "Keychain error \(status): \(SecCopyErrorMessageString(status, nil) ?? "unknown" as CFString)"
        }
    }
}

nonisolated private struct StoredDeviceTokenPair: Codable {
    let keyId: UUID
    let privateKeyRawRepresentation: Data
    let x: String
    let y: String
    let token: XboxDeviceToken
}

actor CredentialStore {
    static let shared = CredentialStore()

    private let service = "\(Bundle.main.bundleIdentifier ?? "dev.tomatopotato.kael").minecraft-accounts"

    private static let credentialAccountPrefix = "credential."
    private static let activeAccountKey = "active-account-id"
    private static let deviceTokenPairKey = "device-token-pair"

    private static let jsonEncoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        return encoder
    }()

    private static let jsonDecoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()

    func allCredentials() throws -> [MinecraftCredentials] {
        try readAllAccounts()
            .filter { $0.account.hasPrefix(Self.credentialAccountPrefix) }
            .compactMap { try? Self.jsonDecoder.decode(MinecraftCredentials.self, from: $0.data) }
    }

    func activeAccountId() throws -> UUID? {
        guard let data = try read(account: Self.activeAccountKey),
              let string = String(data: data, encoding: .utf8) else {
            return nil
        }
        return UUID(uuidString: string)
    }

    func activeCredentials() throws -> MinecraftCredentials? {
        guard let id = try activeAccountId() else {
            return nil
        }
        return try credentials(id: id)
    }

    func credentials(id: UUID) throws -> MinecraftCredentials? {
        guard let data = try read(account: Self.credentialAccountPrefix + id.uuidString) else {
            return nil
        }
        return try Self.jsonDecoder.decode(MinecraftCredentials.self, from: data)
    }

    func upsert(_ credentials: MinecraftCredentials) throws {
        let data = try Self.jsonEncoder.encode(credentials)
        try write(account: Self.credentialAccountPrefix + credentials.id.uuidString, data: data)
    }

    func remove(id: UUID) throws {
        try delete(account: Self.credentialAccountPrefix + id.uuidString)
        if try activeAccountId() == id {
            try delete(account: Self.activeAccountKey)
        }
    }

    func setActive(id: UUID) throws {
        guard let data = id.uuidString.data(using: .utf8) else {
            return
        }
        try write(account: Self.activeAccountKey, data: data)
    }

    func deviceTokenPair() throws -> (key: XboxDeviceTokenKey, token: XboxDeviceToken)? {
        guard let data = try read(account: Self.deviceTokenPairKey) else {
            return nil
        }
        let stored = try Self.jsonDecoder.decode(StoredDeviceTokenPair.self, from: data)
        guard let privateKey = try? P256.Signing.PrivateKey(rawRepresentation: stored.privateKeyRawRepresentation) else {
            return nil
        }
        let key = XboxDeviceTokenKey(id: stored.keyId, privateKey: privateKey, x: stored.x, y: stored.y)
        return (key, stored.token)
    }

    func setDeviceTokenPair(key: XboxDeviceTokenKey, token: XboxDeviceToken) throws {
        let stored = StoredDeviceTokenPair(
            keyId: key.id,
            privateKeyRawRepresentation: key.privateKey.rawRepresentation,
            x: key.x,
            y: key.y,
            token: token
        )
        let data = try Self.jsonEncoder.encode(stored)
        try write(account: Self.deviceTokenPairKey, data: data)
    }

    private func read(account: String) throws -> Data? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]

        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)

        if status == errSecItemNotFound {
            return nil
        }
        guard status == errSecSuccess else {
            throw CredentialStoreError.keychain(status)
        }
        return result as? Data
    }

    private func write(account: String, data: Data) throws {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]

        let existsStatus = SecItemCopyMatching(query as CFDictionary, nil)

        if existsStatus == errSecSuccess {
            let updateStatus = SecItemUpdate(query as CFDictionary, [kSecValueData as String: data] as CFDictionary)
            guard updateStatus == errSecSuccess else {
                throw CredentialStoreError.keychain(updateStatus)
            }
        } else if existsStatus == errSecItemNotFound {
            var addQuery = query
            addQuery[kSecValueData as String] = data
            addQuery[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlock
            let addStatus = SecItemAdd(addQuery as CFDictionary, nil)
            guard addStatus == errSecSuccess else {
                throw CredentialStoreError.keychain(addStatus)
            }
        } else {
            throw CredentialStoreError.keychain(existsStatus)
        }
    }

    private func delete(account: String) throws {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]

        let status = SecItemDelete(query as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw CredentialStoreError.keychain(status)
        }
    }

    private func readAllAccounts() throws -> [(account: String, data: Data)] {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecReturnData as String: true,
            kSecReturnAttributes as String: true,
            kSecMatchLimit as String: kSecMatchLimitAll,
        ]

        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)

        if status == errSecItemNotFound {
            return []
        }
        guard status == errSecSuccess, let items = result as? [[String: Any]] else {
            throw CredentialStoreError.keychain(status)
        }

        return items.compactMap { item in
            guard let account = item[kSecAttrAccount as String] as? String,
                  let data = item[kSecValueData as String] as? Data else {
                return nil
            }
            return (account, data)
        }
    }
}

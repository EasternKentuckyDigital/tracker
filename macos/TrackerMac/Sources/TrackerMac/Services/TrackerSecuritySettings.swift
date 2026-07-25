import Foundation
import Observation
import Security

struct TrackerSyncConfiguration: Sendable {
    let peerURL: String?
    let token: String?
}

enum TrackerCredentialError: LocalizedError {
    case tokenTooShort
    case tokenTooLong
    case invalidToken
    case keychain(OSStatus)

    var errorDescription: String? {
        switch self {
        case .tokenTooShort:
            "The sync token must contain at least 32 UTF-8 bytes."
        case .tokenTooLong:
            "The sync token must not exceed 4,096 UTF-8 bytes."
        case .invalidToken:
            "The sync token cannot contain control characters."
        case let .keychain(status):
            "The sync token could not be saved securely (Keychain error \(status))."
        }
    }
}

@MainActor
@Observable
final class TrackerSecuritySettings {
    @ObservationIgnored
    private let defaults: UserDefaults
    @ObservationIgnored
    private var cachedToken: String?

    var manualPeerURL: String {
        didSet {
            defaults.set(manualPeerURL, forKey: "sync.manualPeerURL")
        }
    }
    private(set) var hasSyncToken: Bool

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        manualPeerURL = (defaults.string(forKey: "sync.manualPeerURL") ?? "")
            .limitedToUTF8Bytes(2_048)
        let token = try? TrackerKeychain.loadToken()
        cachedToken = token
        hasSyncToken = token != nil
    }

    func configuration() -> TrackerSyncConfiguration {
        let peer = manualPeerURL.trimmingCharacters(in: .whitespacesAndNewlines)
        return TrackerSyncConfiguration(
            peerURL: peer.isEmpty ? nil : peer,
            token: cachedToken
        )
    }

    func saveToken(_ token: String) throws {
        let trimmed = token.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty {
            try TrackerKeychain.deleteToken()
            cachedToken = nil
            hasSyncToken = false
            return
        }
        guard trimmed.lengthOfBytes(using: .utf8) >= 32 else {
            throw TrackerCredentialError.tokenTooShort
        }
        guard trimmed.lengthOfBytes(using: .utf8) <= 4_096 else {
            throw TrackerCredentialError.tokenTooLong
        }
        guard !trimmed.unicodeScalars.contains(where: {
            CharacterSet.controlCharacters.contains($0)
        }) else {
            throw TrackerCredentialError.invalidToken
        }
        try TrackerKeychain.saveToken(trimmed)
        cachedToken = trimmed
        hasSyncToken = true
    }
}

private enum TrackerKeychain {
    private static let service =
        Bundle.main.bundleIdentifier ?? "digital.easternkentucky.tracker"
    private static let account = "tailscale-sync-token"

    static func loadToken() throws -> String? {
        var query = baseQuery
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne

        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        if status == errSecItemNotFound {
            return nil
        }
        guard status == errSecSuccess,
              let data = item as? Data,
              let token = String(data: data, encoding: .utf8)
        else {
            throw TrackerCredentialError.keychain(status)
        }
        return token
    }

    static func saveToken(_ token: String) throws {
        let data = Data(token.utf8)
        let updateStatus = SecItemUpdate(
            baseQuery as CFDictionary,
            [kSecValueData as String: data] as CFDictionary
        )
        if updateStatus == errSecSuccess {
            return
        }
        guard updateStatus == errSecItemNotFound else {
            throw TrackerCredentialError.keychain(updateStatus)
        }

        var item = baseQuery
        item[kSecValueData as String] = data
        item[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        let addStatus = SecItemAdd(item as CFDictionary, nil)
        guard addStatus == errSecSuccess else {
            throw TrackerCredentialError.keychain(addStatus)
        }
    }

    static func deleteToken() throws {
        let status = SecItemDelete(baseQuery as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw TrackerCredentialError.keychain(status)
        }
    }

    private static var baseQuery: [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account
        ]
    }
}

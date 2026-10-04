import Foundation
import Security

struct NativeStatus: Decodable, Sendable {
    let running: Bool
    let baseUrl: String?
    let requests: UInt64
}

struct StartOptions: Encodable, Sendable {
    let dataDir: String
    let masterKey: String
    let password: String
    let apiKey: String
    let port: UInt16
    let durationSeconds: Int
}

actor NativeBridge {
    private struct Reply: Decodable {
        let ok: Bool
        let status: NativeStatus?
        let error: String?
    }

    func call(_ command: String, input: StartOptions? = nil) throws -> NativeStatus {
        let text = try input.map { String(decoding: try JSONEncoder().encode($0), as: UTF8.self) } ?? "{}"
        let pointer = command.withCString { name in
            text.withCString { json in gproxy_ios_call(name, json) }
        }
        guard let pointer else { throw failure("Native gateway returned no result") }
        defer { gproxy_ios_free(pointer) }
        let reply = try JSONDecoder().decode(Reply.self, from: Data(String(cString: pointer).utf8))
        guard reply.ok, let status = reply.status else {
            throw failure(reply.error ?? "Native gateway failed")
        }
        return status
    }
}

func failure(_ message: String) -> NSError {
    NSError(domain: "Gproxy", code: 1, userInfo: [NSLocalizedDescriptionKey: message])
}

enum Credentials {
    static let service = "com.leenhawk.gproxy.ios"

    static func secret(_ account: String, prefix: String = "", allowCreate: Bool) throws -> String {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        var lookup = query
        lookup[kSecReturnData as String] = true
        lookup[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(lookup as CFDictionary, &result)
        if status == errSecSuccess, let data = result as? Data,
           let text = String(data: data, encoding: .utf8) { return text }
        guard status == errSecItemNotFound, allowCreate else {
            throw failure("Keychain credential unavailable: \(account) (\(status)). Restore the credential before starting.")
        }
        var bytes = [UInt8](repeating: 0, count: 32)
        guard SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes) == errSecSuccess else {
            throw failure("Secure random generation failed")
        }
        let text = prefix + bytes.map { String(format: "%02x", $0) }.joined()
        var item = query
        item[kSecValueData as String] = Data(text.utf8)
        item[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let added = SecItemAdd(item as CFDictionary, nil)
        guard added == errSecSuccess else { throw failure("Cannot save Keychain credential (\(added))") }
        return text
    }

    static func options(duration: Int) throws -> StartOptions {
        let root = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask,
                                              appropriateFor: nil, create: true).appendingPathComponent("gproxy", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true,
            attributes: [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication])
        let fresh = !FileManager.default.fileExists(atPath: root.appendingPathComponent("gproxy.db").path)
        return try StartOptions(
            dataDir: root.path,
            masterKey: secret("master-key", allowCreate: fresh),
            password: secret("admin-password", allowCreate: fresh),
            apiKey: secret("gateway-key", prefix: "sk-", allowCreate: fresh),
            port: 8787, durationSeconds: duration
        )
    }
}

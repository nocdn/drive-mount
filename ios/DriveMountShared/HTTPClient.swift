import Foundation

struct HTTPClient: Sendable {
    func data(for request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        let (data, response) = try await URLSession.shared.data(for: request)
        guard let httpResponse = response as? HTTPURLResponse else {
            throw RemoteFileError.invalidResponse("Missing HTTP response.")
        }
        guard 200..<300 ~= httpResponse.statusCode else {
            let body = String(data: data.prefix(512), encoding: .utf8) ?? ""
            throw RemoteFileError.server("HTTP \(httpResponse.statusCode) \(body)")
        }
        return (data, httpResponse)
    }

    func download(
        for request: URLRequest,
        suggestedFilename: String? = nil,
        expectedSize: Int64? = nil
    ) async throws -> URL {
        let (url, response) = try await URLSession.shared.download(for: request)
        guard let httpResponse = response as? HTTPURLResponse else {
            throw RemoteFileError.invalidResponse("Missing HTTP response.")
        }
        guard 200..<300 ~= httpResponse.statusCode else {
            throw RemoteFileError.server("HTTP \(httpResponse.statusCode)")
        }

        let sizeNumber = try FileManager.default.attributesOfItem(atPath: url.path)[.size] as? NSNumber
        let fileSize = sizeNumber?.int64Value ?? 0
        if let expectedSize, fileSize != expectedSize {
            throw RemoteFileError.invalidResponse(
                "Downloaded \(fileSize) bytes, but the provider reported \(expectedSize) bytes."
            )
        }

        var destination = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString)
        if let suggestedFilename {
            let pathExtension = (suggestedFilename as NSString).pathExtension
            if !pathExtension.isEmpty {
                destination.appendPathExtension(pathExtension)
            }
        }
        try FileManager.default.moveItem(at: url, to: destination)
        return destination
    }

    func upload(for request: URLRequest, fromFile fileURL: URL) async throws -> (Data, HTTPURLResponse) {
        let (data, response) = try await URLSession.shared.upload(for: request, fromFile: fileURL)
        guard let httpResponse = response as? HTTPURLResponse else {
            throw RemoteFileError.invalidResponse("Missing HTTP response.")
        }
        guard 200..<300 ~= httpResponse.statusCode else {
            let body = String(data: data.prefix(512), encoding: .utf8) ?? ""
            throw RemoteFileError.server("HTTP \(httpResponse.statusCode) \(body)")
        }
        return (data, httpResponse)
    }

    func upload(for request: URLRequest, from data: Data) async throws -> (Data, HTTPURLResponse) {
        let (responseData, response) = try await URLSession.shared.upload(for: request, from: data)
        guard let httpResponse = response as? HTTPURLResponse else {
            throw RemoteFileError.invalidResponse("Missing HTTP response.")
        }
        guard 200..<300 ~= httpResponse.statusCode else {
            let body = String(data: responseData.prefix(512), encoding: .utf8) ?? ""
            throw RemoteFileError.server("HTTP \(httpResponse.statusCode) \(body)")
        }
        return (responseData, httpResponse)
    }
}

extension URLRequest {
    static func jsonPost(url: URL, bearerToken: String? = nil, body: some Encodable) throws -> URLRequest {
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        if let bearerToken {
            request.setValue("Bearer \(bearerToken)", forHTTPHeaderField: "Authorization")
        }
        request.httpBody = try JSONEncoder().encode(body)
        return request
    }

    static func authenticatedGet(url: URL, bearerToken: String) -> URLRequest {
        var request = URLRequest(url: url)
        request.setValue("Bearer \(bearerToken)", forHTTPHeaderField: "Authorization")
        return request
    }
}

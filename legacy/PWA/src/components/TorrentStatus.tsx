export default function TorrentStatus() {
  return (
    <div className="bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-800 rounded-lg p-6">
      <div className="flex items-start space-x-3">
        <div className="flex-shrink-0">
          <svg className="w-6 h-6 text-yellow-600 dark:text-yellow-400" fill="currentColor" viewBox="0 0 20 20">
            <path fillRule="evenodd" d="M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l5.58 9.92c.75 1.334-.213 2.98-1.742 2.98H4.42c-1.53 0-2.493-1.646-1.743-2.98l5.58-9.92zM11 13a1 1 0 11-2 0 1 1 0 012 0zm-1-8a1 1 0 00-1 1v3a1 1 0 002 0V6a1 1 0 00-1-1z" clipRule="evenodd" />
          </svg>
        </div>
        <div className="flex-1">
          <h3 className="text-lg font-medium text-yellow-800 dark:text-yellow-200 mb-2">
            Torrent Client Status
          </h3>
          <div className="space-y-2 text-sm text-yellow-700 dark:text-yellow-300">
            <p>
              <strong>Current State:</strong> No BitTorrent client is currently running in the browser.
            </p>
            <p>
              <strong>Magnet Links:</strong> Your profile generates magnet URIs, but they cannot be seeded/downloaded yet.
            </p>
            <p>
              <strong>Next Steps:</strong> Integration with WebTorrent or similar browser-based torrent library is needed for full P2P functionality.
            </p>
          </div>
          
          <div className="mt-4 p-3 bg-yellow-100 dark:bg-yellow-800/30 rounded-md">
            <h4 className="font-medium text-yellow-800 dark:text-yellow-200 mb-1">
              What Works Now:
            </h4>
            <ul className="text-xs text-yellow-700 dark:text-yellow-300 space-y-1">
              <li>✅ Profile creation with Ed25519 cryptography</li>
              <li>✅ Magnet URI generation</li>
              <li>✅ Profile sharing via clipboard/native share</li>
              <li>❌ Actual torrent seeding/downloading</li>
              <li>❌ Peer discovery and connection</li>
              <li>❌ Distributed profile hosting</li>
            </ul>
          </div>
        </div>
      </div>
    </div>
  )
}
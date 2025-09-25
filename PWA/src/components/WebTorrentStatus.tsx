import { useState, useEffect } from 'react'
import { getTorrentService, TorrentEvent } from '@/lib/torrent'
import ProgressBar from '@/components/ProgressBar'

interface TorrentStats {
  torrents: number
  downloadSpeed: number
  uploadSpeed: number
  downloaded: number
  uploaded: number
  peers: number
}

interface ActiveTorrent {
  infoHash: string
  name: string | null
  magnetURI: string
  progress: number
  downloadSpeed: number
  uploadSpeed: number
  numPeers: number
  downloaded: number
  uploaded: number
}

export default function WebTorrentStatus() {
  const [stats, setStats] = useState<TorrentStats>({
    torrents: 0, downloadSpeed: 0, uploadSpeed: 0, downloaded: 0, uploaded: 0, peers: 0
  })
  const [activeTorrents, setActiveTorrents] = useState<ActiveTorrent[]>([])
  const [events, setEvents] = useState<string[]>([])
  const [isConnected, setIsConnected] = useState(false)

  useEffect(() => {
    let interval: NodeJS.Timeout

    const initializeTorrent = () => {
      try {
        const torrentService = getTorrentService()
        setIsConnected(true)

        // Subscribe to torrent events
        const unsubscribe = torrentService.onEvent((event: TorrentEvent) => {
          const timestamp = new Date().toLocaleTimeString()
          
          switch (event.type) {
            case 'seeding-started':
              if ('profile' in event && event.profile) {
                setEvents(prev => [`[${timestamp}] ✅ Started seeding profile: ${event.profile.username}`, ...prev.slice(0, 9)])
              } else if ('post' in event && event.post) {
                setEvents(prev => [`[${timestamp}] ✅ Started seeding post by: ${event.post.authorDisplayName || 'anonymous'}`, ...prev.slice(0, 9)])
              }
              break
            case 'peer-connected':
              setEvents(prev => [`[${timestamp}] 🔗 Peer connected: ${event.peerId}`, ...prev.slice(0, 9)])
              break
            case 'upload-progress':
              // Don't log every upload progress to avoid spam
              if (Math.random() < 0.1) { // Log ~10% of upload events
                setEvents(prev => [`[${timestamp}] ⬆️ Upload: ${formatBytes(event.uploadSpeed)}/s`, ...prev.slice(0, 9)])
              }
              break
            case 'profile-downloaded':
              if (event.profile) {
                setEvents(prev => [`[${timestamp}] ⬇️ Downloaded profile: ${event.profile.username}`, ...prev.slice(0, 9)])
              }
              break
            case 'download-progress':
              if (Math.random() < 0.1) { // Log ~10% of download events
                setEvents(prev => [`[${timestamp}] ⬇️ Progress: ${(event.progress * 100).toFixed(1)}%`, ...prev.slice(0, 9)])
              }
              break
            case 'error':
              setEvents(prev => [`[${timestamp}] ❌ Error: ${event.error}`, ...prev.slice(0, 9)])
              break
          }
        })

        // Update stats every 2 seconds
        interval = setInterval(() => {
          setStats(torrentService.getStats())
          setActiveTorrents(torrentService.getActiveTorrents())
        }, 2000)

        // Initial update
        setStats(torrentService.getStats())
        setActiveTorrents(torrentService.getActiveTorrents())

        return unsubscribe
      } catch (error) {
        console.error('Failed to initialize torrent service:', error)
        setIsConnected(false)
        setEvents(prev => [`Failed to initialize WebTorrent: ${error}`, ...prev.slice(0, 9)])
      }
    }

    const unsubscribe = initializeTorrent()

    return () => {
      if (interval) clearInterval(interval)
      if (unsubscribe) unsubscribe()
    }
  }, [])

  const formatBytes = (bytes: number): string => {
    if (bytes === 0) return '0 B'
    const k = 1024
    const sizes = ['B', 'KB', 'MB', 'GB']
    const i = Math.floor(Math.log(bytes) / Math.log(k))
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i]
  }

  return (
    <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-xl font-bold text-gray-900 dark:text-white">
          WebTorrent Client
        </h2>
        <div className="flex items-center space-x-2">
          <div className={`w-3 h-3 rounded-full ${isConnected ? 'bg-green-500' : 'bg-red-500'}`}></div>
          <span className="text-sm text-gray-600 dark:text-gray-400">
            {isConnected ? 'Connected' : 'Disconnected'}
          </span>
        </div>
      </div>

      <div className="grid grid-cols-2 md:grid-cols-3 gap-4 mb-6">
        <div className="bg-blue-50 dark:bg-blue-900/20 rounded-lg p-3">
          <div className="text-2xl font-bold text-blue-600 dark:text-blue-400">
            {stats.torrents}
          </div>
          <div className="text-sm text-gray-600 dark:text-gray-400">Active Torrents</div>
        </div>

        <div className="bg-green-50 dark:bg-green-900/20 rounded-lg p-3">
          <div className="text-2xl font-bold text-green-600 dark:text-green-400">
            {stats.peers}
          </div>
          <div className="text-sm text-gray-600 dark:text-gray-400">Connected Peers</div>
        </div>

        <div className="bg-purple-50 dark:bg-purple-900/20 rounded-lg p-3">
          <div className="text-lg font-bold text-purple-600 dark:text-purple-400">
            ⬆️ {formatBytes(stats.uploadSpeed)}/s
          </div>
          <div className="text-sm text-gray-600 dark:text-gray-400">Upload Speed</div>
        </div>

        <div className="bg-orange-50 dark:bg-orange-900/20 rounded-lg p-3">
          <div className="text-lg font-bold text-orange-600 dark:text-orange-400">
            ⬇️ {formatBytes(stats.downloadSpeed)}/s
          </div>
          <div className="text-sm text-gray-600 dark:text-gray-400">Download Speed</div>
        </div>

        <div className="bg-indigo-50 dark:bg-indigo-900/20 rounded-lg p-3">
          <div className="text-lg font-bold text-indigo-600 dark:text-indigo-400">
            {formatBytes(stats.uploaded)}
          </div>
          <div className="text-sm text-gray-600 dark:text-gray-400">Total Uploaded</div>
        </div>

        <div className="bg-pink-50 dark:bg-pink-900/20 rounded-lg p-3">
          <div className="text-lg font-bold text-pink-600 dark:text-pink-400">
            {formatBytes(stats.downloaded)}
          </div>
          <div className="text-sm text-gray-600 dark:text-gray-400">Total Downloaded</div>
        </div>
      </div>

      {activeTorrents.length > 0 && (
        <div className="mb-6">
          <h3 className="text-lg font-semibold text-gray-900 dark:text-white mb-3">
            Active Torrents
          </h3>
          <div className="space-y-2">
            {activeTorrents.map((torrent) => (
              <div key={torrent.infoHash} className="bg-gray-50 dark:bg-gray-700 rounded-lg p-3">
                <div className="flex items-center justify-between mb-2">
                  <div className="font-medium text-gray-900 dark:text-white truncate">
                    {torrent.name || 'Unnamed Torrent'}
                  </div>
                  <div className="text-sm text-gray-600 dark:text-gray-400">
                    {(torrent.progress * 100).toFixed(1)}%
                  </div>
                </div>
                <ProgressBar progress={torrent.progress} className="mb-2" />
                <div className="flex justify-between text-xs text-gray-500 dark:text-gray-400">
                  <span>Peers: {torrent.numPeers}</span>
                  <span>⬆️ {formatBytes(torrent.uploadSpeed)}/s ⬇️ {formatBytes(torrent.downloadSpeed)}/s</span>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      <div>
        <h3 className="text-lg font-semibold text-gray-900 dark:text-white mb-3">
          Recent Activity
        </h3>
        <div className="bg-gray-50 dark:bg-gray-700 rounded-lg p-4 h-48 overflow-y-auto">
          {events.length > 0 ? (
            <div className="space-y-1">
              {events.map((event, index) => (
                <div key={index} className="text-sm font-mono text-gray-700 dark:text-gray-300">
                  {event}
                </div>
              ))}
            </div>
          ) : (
            <div className="text-center text-gray-500 dark:text-gray-400 mt-16">
              No activity yet. Start seeding a profile to see torrent activity!
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
import { useState } from 'react'
import { useProfileStore } from '@/stores/profileStore'
import { getCore } from '@/lib/core'

interface MagnetLinkManagerProps {
  onProfileDownloaded?: (profile: any) => void
}

export default function MagnetLinkManager({ onProfileDownloaded }: MagnetLinkManagerProps) {
  const { currentProfile } = useProfileStore()
  const [magnetInput, setMagnetInput] = useState('')
  const [isDownloading, setIsDownloading] = useState(false)
  const [downloadStatus, setDownloadStatus] = useState<string>('')
  const [myMagnetLink, setMyMagnetLink] = useState<string>('')
  const [isSeeding, setIsSeeding] = useState(false)
  const [showShareModal, setShowShareModal] = useState(false)

  const handleSeedProfile = async () => {
    if (!currentProfile) {
      setDownloadStatus('No profile to seed')
      return
    }

    try {
      setIsSeeding(true)
      setDownloadStatus('Starting to seed your profile...')
      
      const core = await getCore()
      const magnetURI = await core.seedCurrentProfile()
      
      setMyMagnetLink(magnetURI)
      setDownloadStatus(`✅ Profile is now seeding! Share the magnet link with friends.`)
      setShowShareModal(true)
    } catch (error) {
      console.error('Error seeding profile:', error)
      setDownloadStatus(`❌ Failed to seed profile: ${error}`)
    } finally {
      setIsSeeding(false)
    }
  }

  const handleDownloadProfile = async () => {
    if (!magnetInput.trim()) {
      setDownloadStatus('Please enter a magnet link')
      return
    }

    if (!magnetInput.startsWith('magnet:?')) {
      setDownloadStatus('Invalid magnet link format')
      return
    }

    try {
      setIsDownloading(true)
      setDownloadStatus('Connecting to peers and downloading profile...')
      
      const core = await getCore()
      const profile = await core.downloadProfileFromMagnet(magnetInput.trim())
      
      if (profile) {
        setDownloadStatus(`✅ Successfully downloaded profile: ${profile.username}`)
        setMagnetInput('')
        
        // Callback to parent component
        if (onProfileDownloaded) {
          onProfileDownloaded(profile)
        }
      } else {
        setDownloadStatus('❌ Failed to download profile - no data received')
      }
    } catch (error) {
      console.error('Error downloading profile:', error)
      setDownloadStatus(`❌ Download failed: ${error}`)
    } finally {
      setIsDownloading(false)
    }
  }

  const copyMagnetLink = async () => {
    if (myMagnetLink) {
      try {
        await navigator.clipboard.writeText(myMagnetLink)
        setDownloadStatus('✅ Magnet link copied to clipboard!')
        setTimeout(() => setDownloadStatus(''), 3000)
      } catch (error) {
        console.error('Failed to copy magnet link:', error)
        setDownloadStatus('Failed to copy magnet link')
      }
    }
  }

  const shareMagnetLink = async () => {
    if (myMagnetLink && 'share' in navigator) {
      try {
        await navigator.share({
          title: `${currentProfile?.username}'s SnartNet Profile`,
          text: `Connect with me on SnartNet! Download my profile using this magnet link:`,
          url: myMagnetLink
        })
      } catch (error) {
        console.log('Share cancelled or failed:', error)
      }
    }
  }

  return (
    <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
      <h2 className="text-2xl font-bold text-gray-900 dark:text-white mb-6">
        Connect with Friends
      </h2>

      {/* Seed Your Profile Section */}
      <div className="mb-8">
        <h3 className="text-lg font-semibold text-gray-900 dark:text-white mb-3">
          Share Your Profile
        </h3>
        <p className="text-gray-600 dark:text-gray-400 mb-4">
          Start seeding your profile so friends can download it via P2P torrent network.
        </p>
        
        <div className="flex gap-3">
          <button
            onClick={handleSeedProfile}
            disabled={!currentProfile || isSeeding}
            className="px-4 py-2 bg-green-600 hover:bg-green-700 disabled:bg-gray-400 text-white font-medium rounded-lg transition-colors"
          >
            {isSeeding ? 'Starting Seed...' : '🌱 Start Seeding Profile'}
          </button>
          
          {myMagnetLink && (
            <>
              <button
                onClick={copyMagnetLink}
                className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors"
              >
                📋 Copy Magnet Link
              </button>
              
              {'share' in navigator && (
                <button
                  onClick={shareMagnetLink}
                  className="px-4 py-2 bg-purple-600 hover:bg-purple-700 text-white font-medium rounded-lg transition-colors"
                >
                  📱 Share
                </button>
              )}
            </>
          )}
        </div>

        {myMagnetLink && (
          <div className="mt-4 p-3 bg-gray-50 dark:bg-gray-700 rounded-lg">
            <p className="text-sm font-medium text-gray-900 dark:text-white mb-2">
              Your Magnet Link:
            </p>
            <code className="text-xs text-gray-600 dark:text-gray-400 break-all font-mono">
              {myMagnetLink}
            </code>
          </div>
        )}
      </div>

      {/* Download Friend's Profile Section */}
      <div className="mb-6">
        <h3 className="text-lg font-semibold text-gray-900 dark:text-white mb-3">
          Download Friend's Profile
        </h3>
        <p className="text-gray-600 dark:text-gray-400 mb-4">
          Enter a magnet link from a friend to download their profile via P2P.
        </p>
        
        <div className="space-y-3">
          <div>
            <label htmlFor="magnet-input" className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
              Magnet Link
            </label>
            <textarea
              id="magnet-input"
              value={magnetInput}
              onChange={(e) => setMagnetInput(e.target.value)}
              placeholder="magnet:?xt=urn:btih:..."
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent resize-none"
              rows={3}
            />
          </div>
          
          <button
            onClick={handleDownloadProfile}
            disabled={!magnetInput.trim() || isDownloading}
            className="w-full px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-gray-400 text-white font-medium rounded-lg transition-colors"
          >
            {isDownloading ? '⬇️ Downloading...' : '⬇️ Download Profile'}
          </button>
        </div>
      </div>

      {/* Status Messages */}
      {downloadStatus && (
        <div className={`p-4 rounded-lg ${
          downloadStatus.includes('✅') 
            ? 'bg-green-50 dark:bg-green-900/20 text-green-800 dark:text-green-200' 
            : downloadStatus.includes('❌')
            ? 'bg-red-50 dark:bg-red-900/20 text-red-800 dark:text-red-200'
            : 'bg-blue-50 dark:bg-blue-900/20 text-blue-800 dark:text-blue-200'
        }`}>
          <p className="text-sm font-medium">{downloadStatus}</p>
        </div>
      )}

      {/* Share Modal */}
      {showShareModal && myMagnetLink && (
        <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center p-4 z-50">
          <div className="bg-white dark:bg-gray-800 rounded-lg max-w-md w-full p-6">
            <div className="flex justify-between items-center mb-4">
              <h3 className="text-lg font-semibold text-gray-900 dark:text-white">
                🎉 Profile Successfully Seeding!
              </h3>
              <button
                onClick={() => setShowShareModal(false)}
                className="text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
              >
                ✕
              </button>
            </div>
            
            <p className="text-gray-600 dark:text-gray-400 mb-4">
              Your profile is now available on the P2P network. Share this magnet link with friends:
            </p>
            
            <div className="bg-gray-50 dark:bg-gray-700 rounded-lg p-3 mb-4">
              <code className="text-xs text-gray-600 dark:text-gray-400 break-all font-mono">
                {myMagnetLink}
              </code>
            </div>
            
            <div className="flex gap-2">
              <button
                onClick={copyMagnetLink}
                className="flex-1 px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors"
              >
                📋 Copy
              </button>
              
              {'share' in navigator && (
                <button
                  onClick={shareMagnetLink}
                  className="flex-1 px-4 py-2 bg-green-600 hover:bg-green-700 text-white font-medium rounded-lg transition-colors"
                >
                  📱 Share
                </button>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
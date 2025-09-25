import { useEffect, useState } from 'react'
import { getCore } from '@/lib/core'
import { useProfileStore } from '@/stores/profileStore'
import EditProfile from '@/components/EditProfile'

export default function ProfileDisplay() {
  const { currentProfile, setCurrentProfile } = useProfileStore()
  const [publicKey, setPublicKey] = useState<string | null>(null)
  const [fingerprint, setFingerprint] = useState<string | null>(null)
  const [isLoading, setIsLoading] = useState(false)
  const [isDeleting, setIsDeleting] = useState(false)
  const [isEditing, setIsEditing] = useState(false)
  const [extendedProfile, setExtendedProfile] = useState<any>(null)

  useEffect(() => {
    const loadCryptoInfo = async () => {
      if (!currentProfile) return
      
      setIsLoading(true)
      try {
        const core = await getCore()
        const [pk, fp] = await Promise.all([
          core.getPublicKey(),
          core.getFingerprint()
        ])
        setPublicKey(pk)
        setFingerprint(fp)

        // Load extended profile data from localStorage
        const extendedData = localStorage.getItem('snartnet-extended-profile')
        if (extendedData) {
          setExtendedProfile(JSON.parse(extendedData))
        }
      } catch (error) {
        console.error('[ProfileDisplay] Error loading crypto info:', error)
      } finally {
        setIsLoading(false)
      }
    }

    loadCryptoInfo()
  }, [currentProfile])

  if (!currentProfile) {
    return null
  }

  if (isEditing) {
    return (
      <EditProfile
        onCancel={() => setIsEditing(false)}
      />
    )
  }

  const copyToClipboard = async (text: string, label: string) => {
    try {
      await navigator.clipboard.writeText(text)
      alert(`${label} copied to clipboard!`)
    } catch (error) {
      console.error('Failed to copy:', error)
      alert('Failed to copy to clipboard')
    }
  }

  const handleDeleteProfile = async () => {
    if (!confirm('Are you sure you want to delete your profile? This will permanently remove your local keys and profile data.')) {
      return
    }

    setIsDeleting(true)
    try {
      // Clear the profile from the store
      setCurrentProfile(null)
      
      // TODO: Clear local storage/indexedDB
      localStorage.clear()
      
      alert('Profile deleted successfully!')
      
    } catch (error) {
      console.error('Failed to delete profile:', error)
      alert('Failed to delete profile')
    } finally {
      setIsDeleting(false)
    }
  }

  const handleShareMagnet = async () => {
    if (!currentProfile?.magnetUri) {
      alert('No magnet URI available')
      return
    }

    try {
      if (navigator.share) {
        await navigator.share({
          title: `${currentProfile.username}'s SnartNet Profile`,
          text: `Check out my decentralized profile on SnartNet!`,
          url: currentProfile.magnetUri
        })
      } else {
        await copyToClipboard(currentProfile.magnetUri, 'Magnet URI')
      }
    } catch (error) {
      console.error('Failed to share:', error)
      await copyToClipboard(currentProfile.magnetUri, 'Magnet URI')
    }
  }

  return (
    <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-2xl font-bold text-gray-900 dark:text-white">
          Your Profile
        </h2>
        <div className="flex items-center text-green-600 dark:text-green-400">
          <svg className="w-5 h-5 mr-2" fill="currentColor" viewBox="0 0 20 20">
            <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clipRule="evenodd" />
          </svg>
          Active
        </div>
      </div>

      <div className="space-y-4">
        <div>
          <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
            Username
          </label>
          <div className="text-lg font-semibold text-gray-900 dark:text-white">
            {currentProfile.username}
          </div>
        </div>

        {currentProfile.displayName && (
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
              Display Name
            </label>
            <div className="text-lg text-gray-900 dark:text-white">
              {currentProfile.displayName}
            </div>
          </div>
        )}

        {currentProfile.bio && (
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
              Bio
            </label>
            <div className="text-gray-900 dark:text-white">
              {currentProfile.bio}
            </div>
          </div>
        )}

        {extendedProfile?.location && (
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
              Location
            </label>
            <div className="text-gray-900 dark:text-white">
              📍 {extendedProfile.location}
            </div>
          </div>
        )}

        {extendedProfile?.website && (
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
              Website
            </label>
            <div className="text-gray-900 dark:text-white">
              <a 
                href={extendedProfile.website} 
                target="_blank" 
                rel="noopener noreferrer"
                className="text-blue-600 hover:text-blue-700 dark:text-blue-400 dark:hover:text-blue-300"
              >
                🔗 {extendedProfile.website}
              </a>
            </div>
          </div>
        )}

        {extendedProfile?.avatar && (
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
              Avatar
            </label>
            <div className="flex items-center space-x-3">
              <img
                src={extendedProfile.avatar}
                alt="Profile avatar"
                className="w-16 h-16 rounded-full object-cover border-2 border-gray-300 dark:border-gray-600"
                onError={(e) => {
                  const target = e.target as HTMLImageElement
                  target.style.display = 'none'
                }}
              />
              <span className="text-sm text-gray-600 dark:text-gray-400">Profile picture</span>
            </div>
          </div>
        )}

        <div className="border-t border-gray-200 dark:border-gray-700 pt-4">
          <h3 className="text-lg font-medium text-gray-900 dark:text-white mb-3">
            Cryptographic Identity
          </h3>
          
          {isLoading ? (
            <div className="animate-pulse space-y-3">
              <div className="h-4 bg-gray-200 dark:bg-gray-700 rounded w-3/4"></div>
              <div className="h-4 bg-gray-200 dark:bg-gray-700 rounded w-1/2"></div>
            </div>
          ) : (
            <div className="space-y-3">
              {fingerprint && (
                <div>
                  <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                    Fingerprint
                  </label>
                  <div className="flex items-center space-x-2">
                    <code className="text-sm bg-gray-100 dark:bg-gray-700 px-2 py-1 rounded font-mono text-gray-800 dark:text-gray-200 flex-1">
                      {fingerprint}
                    </code>
                    <button
                      onClick={() => copyToClipboard(fingerprint, 'Fingerprint')}
                      className="text-blue-600 hover:text-blue-700 dark:text-blue-400 dark:hover:text-blue-300 text-sm"
                    >
                      Copy
                    </button>
                  </div>
                </div>
              )}

              {publicKey && (
                <div>
                  <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                    Public Key
                  </label>
                  <div className="flex items-center space-x-2">
                    <code className="text-sm bg-gray-100 dark:bg-gray-700 px-2 py-1 rounded font-mono text-gray-800 dark:text-gray-200 flex-1 truncate">
                      {publicKey.substring(0, 32)}...{publicKey.substring(publicKey.length - 8)}
                    </code>
                    <button
                      onClick={() => copyToClipboard(publicKey, 'Public key')}
                      className="text-blue-600 hover:text-blue-700 dark:text-blue-400 dark:hover:text-blue-300 text-sm"
                    >
                      Copy
                    </button>
                  </div>
                </div>
              )}

              {currentProfile.magnetUri && (
                <div>
                  <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                    Magnet URI
                  </label>
                  <div className="flex items-center space-x-2">
                    <code className="text-sm bg-gray-100 dark:bg-gray-700 px-2 py-1 rounded font-mono text-gray-800 dark:text-gray-200 flex-1 truncate">
                      {currentProfile.magnetUri.substring(0, 40)}...
                    </code>
                    <button
                      onClick={() => copyToClipboard(currentProfile.magnetUri!, 'Magnet URI')}
                      className="text-blue-600 hover:text-blue-700 dark:text-blue-400 dark:hover:text-blue-300 text-sm"
                    >
                      Copy
                    </button>
                  </div>
                </div>
              )}
            </div>
          )}
        </div>

        <div className="border-t border-gray-200 dark:border-gray-700 pt-6">
          <h3 className="text-lg font-medium text-gray-900 dark:text-white mb-4">
            Profile Actions
          </h3>
          <div className="flex flex-wrap gap-3">
            <button
              onClick={() => setIsEditing(true)}
              className="inline-flex items-center px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors"
            >
              <svg className="w-4 h-4 mr-2" fill="currentColor" viewBox="0 0 20 20">
                <path d="M13.586 3.586a2 2 0 112.828 2.828l-.793.793-2.828-2.828.793-.793zM11.379 5.793L3 14.172V17h2.828l8.38-8.379-2.83-2.828z" />
              </svg>
              Edit Profile
            </button>

            {currentProfile?.magnetUri && (
              <button
                onClick={handleShareMagnet}
                className="inline-flex items-center px-4 py-2 bg-green-600 hover:bg-green-700 text-white font-medium rounded-lg transition-colors"
              >
                <svg className="w-4 h-4 mr-2" fill="currentColor" viewBox="0 0 20 20">
                  <path d="M15 8a3 3 0 10-2.977-2.63l-4.94 2.47a3 3 0 100 4.319l4.94 2.47a3 3 0 10.895-1.789l-4.94-2.47a3.027 3.027 0 000-.74l4.94-2.47C13.456 7.68 14.19 8 15 8z" />
                </svg>
                Share Profile Magnet Link
              </button>
            )}
            
            <button
              onClick={() => window.location.reload()}
              className="inline-flex items-center px-4 py-2 bg-gray-600 hover:bg-gray-700 text-white font-medium rounded-lg transition-colors"
            >
              <svg className="w-4 h-4 mr-2" fill="currentColor" viewBox="0 0 20 20">
                <path fillRule="evenodd" d="M4 2a1 1 0 011 1v2.101a7.002 7.002 0 0111.601 2.566 1 1 0 11-1.885.666A5.002 5.002 0 005.999 7H9a1 1 0 010 2H4a1 1 0 01-1-1V3a1 1 0 011-1zm.008 9.057a1 1 0 011.276.61A5.002 5.002 0 0014.001 13H11a1 1 0 110-2h5a1 1 0 011 1v5a1 1 0 11-2 0v-2.101a7.002 7.002 0 01-11.601-2.566 1 1 0 01.61-1.276z" clipRule="evenodd" />
              </svg>
              Refresh Profile
            </button>

            <button
              onClick={handleDeleteProfile}
              disabled={isDeleting}
              className="inline-flex items-center px-4 py-2 bg-red-600 hover:bg-red-700 disabled:bg-red-400 text-white font-medium rounded-lg transition-colors disabled:cursor-not-allowed"
            >
              <svg className="w-4 h-4 mr-2" fill="currentColor" viewBox="0 0 20 20">
                <path fillRule="evenodd" d="M9 2a1 1 0 000 2h2a1 1 0 100-2H9z" clipRule="evenodd" />
                <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clipRule="evenodd" />
              </svg>
              {isDeleting ? 'Deleting...' : 'Delete Profile'}
            </button>
          </div>
        </div>

        <div className="bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded-lg p-4">
          <h4 className="text-sm font-medium text-blue-900 dark:text-blue-300 mb-2">
            🔐 Security Information
          </h4>
          <p className="text-xs text-blue-700 dark:text-blue-400 mb-2">
            Your profile is secured with Ed25519 cryptography. Your private keys are stored locally 
            in your browser and never transmitted to any server. The fingerprint uniquely identifies 
            your cryptographic identity across the network.
          </p>
          <p className="text-xs text-blue-700 dark:text-blue-400">
            <strong>Magnet Link:</strong> Your profile can be shared via BitTorrent magnet links for 
            decentralized discovery. Note: Currently there's no torrent client running, so sharing 
            is limited to the link itself.
          </p>
        </div>
      </div>
    </div>
  )
}
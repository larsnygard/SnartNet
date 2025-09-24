import { useEffect, useState } from 'react'
import { getCore } from '@/lib/core'
import { useProfileStore } from '@/stores/profileStore'
import { EditProfile } from './EditProfile'

export default function ProfileDisplay() {
  const { currentProfile } = useProfileStore()
  const [publicKey, setPublicKey] = useState<string | null>(null)
  const [fingerprint, setFingerprint] = useState<string | null>(null)
  const [isLoading, setIsLoading] = useState(false)
  const [isEditing, setIsEditing] = useState(false)
  const [extendedData, setExtendedData] = useState<{
    avatar?: string;
    location?: string;
    website?: string;
  }>({})

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
      } catch (error) {
        console.error('[ProfileDisplay] Error loading crypto info:', error)
      } finally {
        setIsLoading(false)
      }
    }

    loadCryptoInfo()
  }, [currentProfile])

  useEffect(() => {
    if (currentProfile) {
      // Load extended profile data from localStorage
      const extendedDataStr = localStorage.getItem(`profile_extended_${currentProfile.username}`);
      const extended = extendedDataStr ? JSON.parse(extendedDataStr) : {};
      setExtendedData(extended);
    }
  }, [currentProfile])

  if (!currentProfile) {
    return null
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

  return (
    <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-2xl font-bold text-gray-900 dark:text-white">
          Your Profile
        </h2>
        <div className="flex items-center space-x-3">
          <button
            onClick={() => setIsEditing(true)}
            className="px-3 py-1.5 text-sm font-medium text-blue-600 hover:text-blue-700 dark:text-blue-400 dark:hover:text-blue-300 border border-blue-300 hover:border-blue-400 rounded-md hover:bg-blue-50 dark:hover:bg-blue-900/20"
          >
            Edit Profile
          </button>
          <div className="flex items-center text-green-600 dark:text-green-400">
            <svg className="w-5 h-5 mr-2" fill="currentColor" viewBox="0 0 20 20">
              <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clipRule="evenodd" />
            </svg>
            Active
          </div>
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

        {extendedData.avatar && (
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
              Avatar
            </label>
            <img 
              src={extendedData.avatar} 
              alt="Profile avatar" 
              className="w-16 h-16 rounded-full object-cover"
            />
          </div>
        )}

        {extendedData.location && (
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
              Location
            </label>
            <div className="text-gray-900 dark:text-white">
              {extendedData.location}
            </div>
          </div>
        )}

        {extendedData.website && (
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
              Website
            </label>
            <div className="text-gray-900 dark:text-white">
              <a 
                href={extendedData.website} 
                target="_blank" 
                rel="noopener noreferrer"
                className="text-blue-600 hover:text-blue-700 dark:text-blue-400 dark:hover:text-blue-300 underline"
              >
                {extendedData.website}
              </a>
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

        <div className="bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded-lg p-4">
          <h4 className="text-sm font-medium text-blue-900 dark:text-blue-300 mb-2">
            🔐 Security Information
          </h4>
          <p className="text-xs text-blue-700 dark:text-blue-400">
            Your profile is secured with Ed25519 cryptography. Your private keys are stored locally 
            in your browser and never transmitted to any server. The fingerprint uniquely identifies 
            your cryptographic identity across the network.
          </p>
        </div>
      </div>
      
      {isEditing && (
        <div className="mt-6">
          <EditProfile
            onSave={() => {
              setIsEditing(false);
              // Reload extended data after save
              if (currentProfile) {
                const extendedDataStr = localStorage.getItem(`profile_extended_${currentProfile.username}`);
                const extended = extendedDataStr ? JSON.parse(extendedDataStr) : {};
                setExtendedData(extended);
              }
            }}
            onCancel={() => setIsEditing(false)}
          />
        </div>
      )}
    </div>
  )
}
import { useState, useEffect } from 'react'
import MagnetLinkManager from '@/components/MagnetLinkManager'
import { useContactStore, type RelationshipType } from '@/stores/contactStore'

interface DiscoveredProfile {
  id: string
  username: string
  displayName: string
  bio?: string
  publicKey: string
  avatar?: string
  downloadedAt: string
  magnetURI?: string
}

export default function DiscoverPage() {
  const [discoveredProfiles, setDiscoveredProfiles] = useState<DiscoveredProfile[]>([])
  const { addContact, getContact } = useContactStore()

  // Load discovered profiles from localStorage on mount
  useEffect(() => {
    const saved = localStorage.getItem('snartnet-discovered-profiles')
    if (saved) {
      try {
        setDiscoveredProfiles(JSON.parse(saved))
      } catch (error) {
        console.error('Failed to load discovered profiles:', error)
      }
    }
  }, [])

  // Save discovered profiles to localStorage whenever it changes
  useEffect(() => {
    localStorage.setItem('snartnet-discovered-profiles', JSON.stringify(discoveredProfiles))
  }, [discoveredProfiles])

  const handleProfileDownloaded = (profile: any) => {
    // Check if profile already exists
    const existingIndex = discoveredProfiles.findIndex(p => p.publicKey === profile.publicKey)
    
    const discoveredProfile: DiscoveredProfile = {
      id: profile.publicKey.slice(0, 8), // Use first 8 chars of public key as ID
      username: profile.username,
      displayName: profile.displayName || profile.username,
      bio: profile.bio,
      publicKey: profile.publicKey,
      avatar: profile.avatar,
      downloadedAt: new Date().toISOString(),
      magnetURI: profile.magnetURI
    }

    if (existingIndex >= 0) {
      // Update existing profile
      const updated = [...discoveredProfiles]
      updated[existingIndex] = { ...updated[existingIndex], ...discoveredProfile }
      setDiscoveredProfiles(updated)
    } else {
      // Add new profile
      setDiscoveredProfiles(prev => [discoveredProfile, ...prev])
    }
  }

  const removeProfile = (publicKey: string) => {
    setDiscoveredProfiles(prev => prev.filter(p => p.publicKey !== publicKey))
  }

  const addAsContact = (profile: DiscoveredProfile, relationship: RelationshipType = 'friend') => {
    // Check if already a contact
    const contactId = `contact_${btoa(profile.username + (profile.magnetURI || '')).slice(0, 16)}`
    const existingContact = getContact(contactId)
    
    if (existingContact) {
      alert(`${profile.username} is already in your contacts!`)
      return
    }

    addContact({
      username: profile.username,
      displayName: profile.displayName,
      relationship: relationship,
      trustLevel: relationship === 'friend' ? 7 : 5,
      magnetUri: profile.magnetURI || '',
      avatar: profile.avatar,
      notes: `Added from discovery on ${new Date().toLocaleDateString()}`
    })

    alert(`${profile.username} has been added to your ${relationship}s!`)
  }

  const formatDate = (isoString: string) => {
    return new Date(isoString).toLocaleDateString() + ' ' + new Date(isoString).toLocaleTimeString()
  }

  return (
    <div className="space-y-8">
      <div>
        <h1 className="text-3xl font-bold text-gray-900 dark:text-white mb-2">
          Discover Friends
        </h1>
        <p className="text-gray-600 dark:text-gray-400">
          Share your profile and download friends' profiles using magnet links.
        </p>
      </div>

      {/* Magnet Link Manager */}
      <MagnetLinkManager onProfileDownloaded={handleProfileDownloaded} />

      {/* Discovered Profiles */}
      <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
        <div className="flex justify-between items-center mb-6">
          <h2 className="text-xl font-bold text-gray-900 dark:text-white">
            Downloaded Friends ({discoveredProfiles.length})
          </h2>
        </div>

        {discoveredProfiles.length === 0 ? (
          <div className="text-center py-8">
            <div className="text-6xl mb-4">👥</div>
            <h3 className="text-lg font-medium text-gray-900 dark:text-white mb-2">
              No friends discovered yet
            </h3>
            <p className="text-gray-600 dark:text-gray-400">
              Use the magnet link manager above to download friends' profiles from the P2P network.
            </p>
          </div>
        ) : (
          <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
            {discoveredProfiles.map((profile) => (
              <div
                key={profile.publicKey}
                className="border border-gray-200 dark:border-gray-600 rounded-lg p-4 hover:border-blue-300 dark:hover:border-blue-500 transition-colors"
              >
                <div className="flex items-center justify-between mb-3">
                  <div className="flex items-center space-x-3">
                    {profile.avatar ? (
                      <img
                        src={profile.avatar}
                        alt={`${profile.username}'s avatar`}
                        className="w-10 h-10 rounded-full"
                      />
                    ) : (
                      <div className="w-10 h-10 bg-gradient-to-br from-blue-400 to-purple-500 rounded-full flex items-center justify-center text-white font-bold">
                        {profile.username.charAt(0).toUpperCase()}
                      </div>
                    )}
                    <div>
                      <h3 className="font-semibold text-gray-900 dark:text-white">
                        {profile.displayName}
                      </h3>
                      <p className="text-sm text-gray-600 dark:text-gray-400">
                        @{profile.username}
                      </p>
                    </div>
                  </div>
                  
                  <button
                    onClick={() => removeProfile(profile.publicKey)}
                    className="text-red-500 hover:text-red-700 dark:text-red-400 dark:hover:text-red-300 p-1"
                    title="Remove profile"
                  >
                    🗑️
                  </button>
                </div>

                {profile.bio && (
                  <p className="text-sm text-gray-700 dark:text-gray-300 mb-3 line-clamp-2">
                    {profile.bio}
                  </p>
                )}

                <div className="text-xs text-gray-500 dark:text-gray-400 mb-3">
                  <p>Downloaded: {formatDate(profile.downloadedAt)}</p>
                  <p className="font-mono">ID: {profile.id}</p>
                </div>

                <div className="flex gap-2 mb-2">
                  <button
                    onClick={() => addAsContact(profile, 'friend')}
                    className="flex-1 px-3 py-2 bg-green-600 hover:bg-green-700 text-white text-sm font-medium rounded-lg transition-colors"
                  >
                    ➕ Add Friend
                  </button>
                  <button
                    onClick={() => addAsContact(profile, 'acquaintance')}
                    className="flex-1 px-3 py-2 bg-blue-600 hover:bg-blue-700 text-white text-sm font-medium rounded-lg transition-colors"
                  >
                    🤝 Add Acquaintance
                  </button>
                </div>
                
                <div className="flex gap-2">
                  <button className="flex-1 px-3 py-2 bg-gray-600 hover:bg-gray-700 text-white text-sm font-medium rounded-lg transition-colors">
                    💬 Message
                  </button>
                  <button className="flex-1 px-3 py-2 bg-gray-600 hover:bg-gray-700 text-white text-sm font-medium rounded-lg transition-colors">
                    👤 View Profile
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
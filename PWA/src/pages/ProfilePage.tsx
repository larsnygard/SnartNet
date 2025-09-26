import { useState } from 'react'
import { useProfileStore } from '@/stores/profileStore'
import { getCore } from '@/lib/core'
import CreateProfile from '@/components/CreateProfile'
import ProfileDisplay from '@/components/ProfileDisplay'
import EditProfile from '@/components/EditProfile'
import WebTorrentStatus from '@/components/WebTorrentStatus'
import BackupRestore from '@/components/BackupRestore'


const ProfilePage: React.FC = () => {
  const { currentProfile, loading, seedProfileEnabled, setSeedProfileEnabled } = useProfileStore()
  const [showCreateNew, setShowCreateNew] = useState(false)
  const [showBackupRestore, setShowBackupRestore] = useState(false)
  const [autoSeedStatus, setAutoSeedStatus] = useState<string>('')
  const [showEditProfile, setShowEditProfile] = useState(false)

  // Auto seed when toggle ON and profile exists
  async function ensureSeeding() {
    if (!currentProfile || !seedProfileEnabled) return
    try {
      setAutoSeedStatus('Seeding profile...')
      const core = await getCore()
      const magnet = await core.seedCurrentProfile()
      setAutoSeedStatus(`Seeding ✓ (${magnet.slice(0,40)}...)`)
    } catch (e:any) {
      setAutoSeedStatus(`Seeding failed: ${e?.message || 'error'}`)
    }
  }

  if (currentProfile && seedProfileEnabled && autoSeedStatus === '') {
    // fire and forget (simple guard to avoid loops)
    ensureSeeding()
  }

  return (
    <div>
      <div className="flex justify-between items-center mb-6">
        <h1 className="text-3xl font-bold text-gray-900 dark:text-white">
          Profile Management
        </h1>
        
        <div className="flex gap-4 items-center">
          <label className="flex items-center gap-2 text-sm text-gray-700 dark:text-gray-300 cursor-pointer select-none">
            <input
              type="checkbox"
              className="rounded border-gray-300 dark:border-gray-600 text-blue-600 focus:ring-blue-500"
              checked={seedProfileEnabled}
              onChange={e => {
                setSeedProfileEnabled(e.target.checked)
                if (e.target.checked) {
                  ensureSeeding()
                }
              }}
            />
            <span>Seed my profile (auto)</span>
            {autoSeedStatus && (
              <span className="text-xs text-gray-500 dark:text-gray-400">{autoSeedStatus}</span>
            )}
          </label>
          {currentProfile && !showCreateNew && !showBackupRestore && !showEditProfile && (
            <>
              <button
                onClick={() => setShowBackupRestore(true)}
                className="px-4 py-2 bg-purple-600 hover:bg-purple-700 text-white font-medium rounded-lg transition-colors"
              >
                💾 Backup & Restore
              </button>
              <button
                onClick={() => setShowCreateNew(true)}
                className="px-4 py-2 bg-green-600 hover:bg-green-700 text-white font-medium rounded-lg transition-colors"
              >
                Create New Profile
              </button>
              <button
                onClick={() => setShowEditProfile(true)}
                className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors"
              >
                Edit Profile
              </button>
            </>
          )}
        </div>
      </div>
      
      {loading && (
        <div className="flex items-center justify-center py-8">
          <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-600"></div>
          <span className="ml-3 text-gray-600 dark:text-gray-400">Loading profile...</span>
        </div>
      )}

      {!loading && (
        <div className="space-y-8">
          {showBackupRestore && (
            <div>
              <BackupRestore onClose={() => setShowBackupRestore(false)} />
            </div>
          )}

          {showCreateNew && !showBackupRestore && !showEditProfile && (
            <div>
              <div className="flex items-center justify-between mb-4">
                <h2 className="text-xl font-semibold text-gray-900 dark:text-white">
                  Create New Profile
                </h2>
                <button
                  onClick={() => setShowCreateNew(false)}
                  className="text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
                >
                  ✕ Cancel
                </button>
              </div>
              <CreateProfile />
            </div>
          )}
          
          {!showBackupRestore && !showEditProfile && (
            <>
              {currentProfile ? (
                <div>
                  {!showCreateNew && (
                    <>
                      <h2 className="text-xl font-semibold text-gray-900 dark:text-white mb-4">
                        Current Profile
                      </h2>
                      <ProfileDisplay />
                    </>
                  )}
                </div>
              ) : (
                !showCreateNew && <CreateProfile />
              )}
              
              <WebTorrentStatus />
            </>
          )}
          {showEditProfile && currentProfile && !showBackupRestore && !showCreateNew && (
            <div className="space-y-4">
              <div className="flex items-center justify-between mb-2">
                <h2 className="text-xl font-semibold text-gray-900 dark:text-white">Edit Profile</h2>
                <button
                  onClick={() => setShowEditProfile(false)}
                  className="text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
                >
                  ✕ Close
                </button>
              </div>
              <EditProfile onCancel={() => setShowEditProfile(false)} />
            </div>
          )}
        </div>
      )}
    </div>
  )
}

export default ProfilePage
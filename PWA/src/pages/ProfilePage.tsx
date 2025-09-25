import { useState } from 'react'
import { useProfileStore } from '@/stores/profileStore'
import CreateProfile from '@/components/CreateProfile'
import ProfileDisplay from '@/components/ProfileDisplay'
import WebTorrentStatus from '@/components/WebTorrentStatus'
import MagnetLinkManager from '@/components/MagnetLinkManager'
import BackupRestore from '@/components/BackupRestore'


const ProfilePage: React.FC = () => {
  const { currentProfile, loading } = useProfileStore()
  const [showCreateNew, setShowCreateNew] = useState(false)
  const [showBackupRestore, setShowBackupRestore] = useState(false)

  return (
    <div>
      <div className="flex justify-between items-center mb-6">
        <h1 className="text-3xl font-bold text-gray-900 dark:text-white">
          Profile Management
        </h1>
        
        <div className="flex gap-2">
          {currentProfile && !showCreateNew && !showBackupRestore && (
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

          {showCreateNew && !showBackupRestore && (
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
          
          {!showBackupRestore && (
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
              
              <MagnetLinkManager />
              <WebTorrentStatus />
            </>
          )}
        </div>
      )}
    </div>
  )
}

export default ProfilePage
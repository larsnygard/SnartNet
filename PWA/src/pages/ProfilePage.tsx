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
  const [showResetConfirm, setShowResetConfirm] = useState(false)
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
                onClick={() => setShowEditProfile(true)}
                className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors"
              >
                Edit Profile
              </button>
              <button
                onClick={() => setShowResetConfirm(true)}
                className="px-4 py-2 bg-red-600 hover:bg-red-700 text-white font-medium rounded-lg transition-colors"
              >
                Reset / Delete All
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

          {showCreateNew && !showBackupRestore && !showEditProfile && !currentProfile && (
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
      {showResetConfirm && (
        <ResetConfirmModal onClose={() => setShowResetConfirm(false)} />
      )}
    </div>
  )
}

import { resetApplicationState } from '@/lib/resetApp'

interface ResetConfirmProps { onClose: () => void }
const ResetConfirmModal: React.FC<ResetConfirmProps> = ({ onClose }) => {
  const [busy, setBusy] = useState(false)
  const [done, setDone] = useState(false)
  async function handleConfirm() {
    setBusy(true)
    await resetApplicationState()
    setBusy(false)
    setDone(true)
  }
  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-white dark:bg-gray-800 p-6 rounded-lg w-full max-w-md space-y-4">
        <div className="flex items-center justify-between">
          <h3 className="text-lg font-semibold text-gray-900 dark:text-white">Reset / Delete All Data</h3>
          <button onClick={onClose} className="text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200">✕</button>
        </div>
        {!done && (
          <p className="text-sm text-gray-600 dark:text-gray-400">This will permanently clear your profile, contacts, posts, and local files. You cannot undo this action. Make a backup first if you need to keep anything.</p>
        )}
        {done && (
          <p className="text-sm text-green-700 dark:text-green-300">All data cleared. Reload the page to start fresh.</p>
        )}
        <div className="flex justify-end gap-3">
          {!done && (
            <>
              <button onClick={onClose} disabled={busy} className="px-4 py-2 rounded border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 bg-white dark:bg-gray-700 hover:bg-gray-50 dark:hover:bg-gray-600 disabled:opacity-50">Cancel</button>
              <button onClick={handleConfirm} disabled={busy} className="px-4 py-2 rounded bg-red-600 hover:bg-red-700 text-white font-medium disabled:opacity-50 flex items-center gap-2">
                {busy && <span className="animate-spin h-4 w-4 border-b-2 border-white rounded-full"></span>}
                {busy ? 'Resetting…' : 'Yes, Delete Everything'}
              </button>
            </>
          )}
          {done && (
            <button onClick={() => { onClose(); location.reload() }} className="px-4 py-2 rounded bg-blue-600 hover:bg-blue-700 text-white font-medium">Reload</button>
          )}
        </div>
      </div>
    </div>
  )
}

export default ProfilePage
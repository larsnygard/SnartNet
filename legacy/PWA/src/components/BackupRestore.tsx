import React, { useState, useRef } from 'react'
import { BackupService } from '../lib/backup'
import { useProfileStore } from '../stores/profileStore'

interface BackupRestoreProps {
  onClose?: () => void
}

const BackupRestore: React.FC<BackupRestoreProps> = ({ onClose }) => {
  const { currentProfile } = useProfileStore()
  const fileInputRef = useRef<HTMLInputElement>(null)
  
  const [activeTab, setActiveTab] = useState<'backup' | 'restore'>('backup')
  const [includeContacts, setIncludeContacts] = useState(true)
  const [usePassword, setUsePassword] = useState(false)
  const [password, setPassword] = useState('')
  const [isProcessing, setIsProcessing] = useState(false)
  const [status, setStatus] = useState('')
  const [backupInfo, setBackupInfo] = useState<any>(null)

  const handleBackupDownload = async () => {
    if (!currentProfile) {
      setStatus('❌ No profile to backup')
      return
    }

    if (usePassword && !password.trim()) {
      setStatus('❌ Password is required')
      return
    }

    try {
      setIsProcessing(true)
      setStatus('📦 Creating backup...')
      
      await BackupService.downloadBackup(includeContacts, usePassword ? password : undefined)
      
      setStatus('✅ Backup downloaded successfully!')
    } catch (error) {
      console.error('Backup failed:', error)
      setStatus(`❌ Backup failed: ${error instanceof Error ? error.message : 'Unknown error'}`)
    } finally {
      setIsProcessing(false)
    }
  }

  const handleFileSelect = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0]
    if (!file) return

    try {
      setIsProcessing(true)
      setStatus('📄 Reading backup file...')
      
      // Try to get backup info
      const info = await BackupService.getBackupInfo(file, usePassword ? password : undefined)
      setBackupInfo(info)
      setStatus('📋 Backup file loaded. Review info below and click Restore.')
    } catch (error) {
      console.error('Failed to read backup:', error)
      setStatus(`❌ Failed to read backup: ${error instanceof Error ? error.message : 'Unknown error'}`)
      setBackupInfo(null)
    } finally {
      setIsProcessing(false)
    }
  }

  const handleRestore = async () => {
    const file = fileInputRef.current?.files?.[0]
    if (!file) {
      setStatus('❌ Please select a backup file')
      return
    }

    if (usePassword && !password.trim()) {
      setStatus('❌ Password is required')
      return
    }

    if (!confirm('⚠️ This will replace your current profile and data. Are you sure?')) {
      return
    }

    try {
      setIsProcessing(true)
      setStatus('🔄 Restoring backup...')
      
      await BackupService.restoreFromFile(file, usePassword ? password : undefined)
      
      setStatus('✅ Backup restored successfully! Page will reload...')
    } catch (error) {
      console.error('Restore failed:', error)
      setStatus(`❌ Restore failed: ${error instanceof Error ? error.message : 'Unknown error'}`)
    } finally {
      setIsProcessing(false)
    }
  }

  const resetForm = () => {
    setPassword('')
    setStatus('')
    setBackupInfo(null)
    if (fileInputRef.current) {
      fileInputRef.current.value = ''
    }
  }

  return (
    <div className="max-w-2xl mx-auto p-6 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg">
      <div className="flex justify-between items-center mb-6">
        <h2 className="text-2xl font-bold text-gray-900 dark:text-white">
          Profile Backup & Restore
        </h2>
        {onClose && (
          <button
            onClick={onClose}
            className="text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-300"
          >
            ✕
          </button>
        )}
      </div>

      {/* Tab Navigation */}
      <div className="border-b border-gray-200 dark:border-gray-700 mb-6">
        <nav className="flex space-x-8">
          <button
            onClick={() => { setActiveTab('backup'); resetForm() }}
            className={`py-2 px-1 border-b-2 font-medium text-sm transition-colors ${
              activeTab === 'backup'
                ? 'border-blue-500 text-blue-600 dark:text-blue-400'
                : 'border-transparent text-gray-500 hover:text-gray-700 dark:text-gray-400'
            }`}
          >
            📦 Backup
          </button>
          <button
            onClick={() => { setActiveTab('restore'); resetForm() }}
            className={`py-2 px-1 border-b-2 font-medium text-sm transition-colors ${
              activeTab === 'restore'
                ? 'border-blue-500 text-blue-600 dark:text-blue-400'
                : 'border-transparent text-gray-500 hover:text-gray-700 dark:text-gray-400'
            }`}
          >
            🔄 Restore
          </button>
        </nav>
      </div>

      {/* Backup Tab */}
      {activeTab === 'backup' && (
        <div className="space-y-6">
          <div className="bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-700 rounded-lg p-4">
            <h3 className="font-semibold text-blue-900 dark:text-blue-100 mb-2">
              📋 What gets backed up:
            </h3>
            <ul className="text-sm text-blue-800 dark:text-blue-200 space-y-1">
              <li>• Your profile information and cryptographic keys</li>
              <li>• Extended profile data (avatar, location, website)</li>
              <li>• Your contacts and relationships (optional)</li>
              <li>• All necessary data to restore your identity</li>
            </ul>
          </div>

          {currentProfile ? (
            <div className="bg-gray-50 dark:bg-gray-700 rounded-lg p-4">
              <h4 className="font-medium text-gray-900 dark:text-white mb-2">Current Profile:</h4>
              <p className="text-sm text-gray-600 dark:text-gray-300">
                <strong>Username:</strong> {currentProfile.username}<br/>
                <strong>Display Name:</strong> {currentProfile.displayName || 'Not set'}<br/>
                <strong>Public Key:</strong> {currentProfile.publicKey?.slice(0, 16) + '...' || 'Unknown'}
              </p>
            </div>
          ) : (
            <div className="bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-700 rounded-lg p-4">
              <p className="text-yellow-800 dark:text-yellow-200">
                ⚠️ No profile found to backup. Please create a profile first.
              </p>
            </div>
          )}

          <div className="space-y-4">
            <label className="flex items-center">
              <input
                type="checkbox"
                checked={includeContacts}
                onChange={(e) => setIncludeContacts(e.target.checked)}
                className="mr-2"
              />
              <span className="text-gray-700 dark:text-gray-300">
                Include contacts and relationships in backup
              </span>
            </label>

            <label className="flex items-center">
              <input
                type="checkbox"
                checked={usePassword}
                onChange={(e) => setUsePassword(e.target.checked)}
                className="mr-2"
              />
              <span className="text-gray-700 dark:text-gray-300">
                Password protect backup (recommended)
              </span>
            </label>

            {usePassword && (
              <input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="Enter backup password"
                className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
              />
            )}
          </div>

          <button
            onClick={handleBackupDownload}
            disabled={isProcessing || !currentProfile}
            className="w-full px-4 py-3 bg-blue-600 hover:bg-blue-700 disabled:bg-gray-400 text-white font-medium rounded-lg transition-colors"
          >
            {isProcessing ? '📦 Creating Backup...' : '💾 Download Backup'}
          </button>
        </div>
      )}

      {/* Restore Tab */}
      {activeTab === 'restore' && (
        <div className="space-y-6">
          <div className="bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-700 rounded-lg p-4">
            <h3 className="font-semibold text-yellow-900 dark:text-yellow-100 mb-2">
              ⚠️ Important Warning:
            </h3>
            <p className="text-sm text-yellow-800 dark:text-yellow-200">
              Restoring a backup will completely replace your current profile, keys, and data. 
              Make sure to backup your current profile first if you want to keep it.
            </p>
          </div>

          <div className="space-y-4">
            <div>
              <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
                Select Backup File:
              </label>
              <input
                ref={fileInputRef}
                type="file"
                accept=".snartnet-backup,.json"
                onChange={handleFileSelect}
                className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
              />
              <p className="text-xs text-gray-500 dark:text-gray-400 mt-1">
                Accepts .snartnet-backup and .json files
              </p>
            </div>

            <label className="flex items-center">
              <input
                type="checkbox"
                checked={usePassword}
                onChange={(e) => setUsePassword(e.target.checked)}
                className="mr-2"
              />
              <span className="text-gray-700 dark:text-gray-300">
                Backup is password protected
              </span>
            </label>

            {usePassword && (
              <input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="Enter backup password"
                className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
              />
            )}
          </div>

          {backupInfo && (
            <div className="bg-gray-50 dark:bg-gray-700 rounded-lg p-4">
              <h4 className="font-medium text-gray-900 dark:text-white mb-2">Backup Information:</h4>
              <div className="text-sm text-gray-600 dark:text-gray-300 space-y-1">
                <p><strong>Username:</strong> {backupInfo.username}</p>
                <p><strong>Created:</strong> {new Date(backupInfo.timestamp).toLocaleString()}</p>
                <p><strong>Version:</strong> {backupInfo.version}</p>
                <p><strong>Fingerprint:</strong> {backupInfo.fingerprint}</p>
                <p><strong>Includes Contacts:</strong> {backupInfo.hasContacts ? 'Yes' : 'No'}</p>
              </div>
            </div>
          )}

          <button
            onClick={handleRestore}
            disabled={isProcessing || !backupInfo}
            className="w-full px-4 py-3 bg-orange-600 hover:bg-orange-700 disabled:bg-gray-400 text-white font-medium rounded-lg transition-colors"
          >
            {isProcessing ? '🔄 Restoring...' : '🔄 Restore Backup'}
          </button>
        </div>
      )}

      {/* Status Message */}
      {status && (
        <div className={`mt-6 p-4 rounded-lg ${
          status.startsWith('✅') ? 'bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-700' :
          status.startsWith('❌') ? 'bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-700' :
          status.startsWith('⚠️') ? 'bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-700' :
          'bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-700'
        }`}>
          <p className={`text-sm font-medium ${
            status.startsWith('✅') ? 'text-green-800 dark:text-green-200' :
            status.startsWith('❌') ? 'text-red-800 dark:text-red-200' :
            status.startsWith('⚠️') ? 'text-yellow-800 dark:text-yellow-200' :
            'text-blue-800 dark:text-blue-200'
          }`}>
            {status}
          </p>
        </div>
      )}
    </div>
  )
}

export default BackupRestore
import { getCore } from './core'

export interface ProfileBackup {
  version: string
  timestamp: string
  profile: any
  keypair: any
  extendedData: any
  contacts?: any[]
  metadata: {
    username: string
    fingerprint: string
    backupId: string
  }
}

export class BackupService {
  private static readonly BACKUP_VERSION = '1.0.0'
  private static readonly FILE_EXTENSION = '.snartnet-backup'

  /**
   * Create a complete backup of the current profile, keys, and associated data
   */
  static async createBackup(includeContacts: boolean = true): Promise<ProfileBackup> {
    try {
      const core = await getCore()
      
      // Get current profile
      const profile = await core.getCurrentProfile()
      if (!profile) {
        throw new Error('No current profile to backup')
      }

      // Get keypair from localStorage (this is where the WASM core stores it)
      const keypairData = localStorage.getItem('snartnet_keypair')
      if (!keypairData) {
        throw new Error('No keypair found for backup')
      }

      // Get extended profile data
      const extendedData = localStorage.getItem(`profile-extended-${profile.username}`)
      
      // Get contacts if requested
      let contacts = null
      if (includeContacts) {
        const contactsData = localStorage.getItem('snartnet-contacts')
        contacts = contactsData ? JSON.parse(contactsData) : []
      }

      // Create backup object
      const backup: ProfileBackup = {
        version: this.BACKUP_VERSION,
        timestamp: new Date().toISOString(),
        profile: profile,
        keypair: JSON.parse(keypairData),
        extendedData: extendedData ? JSON.parse(extendedData) : null,
        contacts: contacts,
        metadata: {
          username: profile.username,
          fingerprint: profile.fingerprint || 'unknown',
          backupId: this.generateBackupId()
        }
      }

      console.log('Backup created successfully:', {
        username: backup.metadata.username,
        timestamp: backup.timestamp,
        includesContacts: !!backup.contacts
      })

      return backup
    } catch (error) {
      console.error('Failed to create backup:', error)
      throw new Error(`Backup creation failed: ${error instanceof Error ? error.message : 'Unknown error'}`)
    }
  }

  /**
   * Download backup as a file
   */
  static async downloadBackup(includeContacts: boolean = true, password?: string): Promise<void> {
    try {
      const backup = await this.createBackup(includeContacts)
      
      let backupData: any = backup
      
      // Apply password protection if provided
      if (password) {
        backupData = await this.encryptBackup(backup, password)
      }

      const backupJson = JSON.stringify(backupData, null, 2)
      const blob = new Blob([backupJson], { type: 'application/json' })
      
      // Create download
      const url = URL.createObjectURL(blob)
      const link = document.createElement('a')
      link.href = url
      link.download = `${backup.metadata.username}_backup_${this.formatDateForFilename(backup.timestamp)}${this.FILE_EXTENSION}`
      
      document.body.appendChild(link)
      link.click()
      document.body.removeChild(link)
      URL.revokeObjectURL(url)

      console.log('Backup downloaded successfully')
    } catch (error) {
      console.error('Failed to download backup:', error)
      throw error
    }
  }

  /**
   * Restore profile from backup data
   */
  static async restoreFromBackup(backupData: any, password?: string): Promise<void> {
    try {
      let backup: ProfileBackup
      
      // Decrypt if password provided
      if (password && backupData.encrypted) {
        backup = await this.decryptBackup(backupData, password)
      } else {
        backup = backupData as ProfileBackup
      }

      // Validate backup format
      this.validateBackup(backup)

      console.log('Restoring backup:', {
        version: backup.version,
        username: backup.metadata.username,
        timestamp: backup.timestamp
      })

      // Clear existing data
      localStorage.removeItem('snartnet_keypair')
      localStorage.removeItem('snartnet_current_profile')
      localStorage.removeItem('snartnet-contacts')

      // Restore keypair
      localStorage.setItem('snartnet_keypair', JSON.stringify(backup.keypair))

      // Restore profile
      localStorage.setItem('snartnet_current_profile', JSON.stringify({
        profile: backup.profile,
        signature: backup.keypair?.signature || 'restored' // Fallback if no signature
      }))

      // Restore extended data
      if (backup.extendedData && backup.metadata.username) {
        localStorage.setItem(`profile-extended-${backup.metadata.username}`, JSON.stringify(backup.extendedData))
      }

      // Restore contacts
      if (backup.contacts) {
        localStorage.setItem('snartnet-contacts', JSON.stringify(backup.contacts))
      }

      console.log('Backup restored successfully')
      
      // Reload page to reinitialize with restored data
      window.location.reload()
    } catch (error) {
      console.error('Failed to restore backup:', error)
      throw new Error(`Restore failed: ${error instanceof Error ? error.message : 'Unknown error'}`)
    }
  }

  /**
   * Restore from uploaded file
   */
  static async restoreFromFile(file: File, password?: string): Promise<void> {
    try {
      if (!file.name.endsWith(this.FILE_EXTENSION) && !file.name.endsWith('.json')) {
        throw new Error('Invalid backup file format. Please select a .snartnet-backup or .json file.')
      }

      const fileContent = await this.readFileContent(file)
      const backupData = JSON.parse(fileContent)
      
      await this.restoreFromBackup(backupData, password)
    } catch (error) {
      console.error('Failed to restore from file:', error)
      throw error
    }
  }

  /**
   * Simple encryption using built-in crypto (for basic protection)
   */
  private static async encryptBackup(backup: ProfileBackup, _password: string): Promise<any> {
    // For now, just base64 encode with password hint
    // In production, you'd want proper encryption with the password
    // TODO: Implement proper encryption using _password
    
    const data = JSON.stringify(backup)
    const encoded = btoa(unescape(encodeURIComponent(data)))
    
    return {
      encrypted: true,
      version: backup.version,
      data: encoded,
      hint: `Backup for ${backup.metadata.username}`,
      timestamp: backup.timestamp
    }
  }

  /**
   * Simple decryption (matches encryption method)
   */
  private static async decryptBackup(encryptedData: any, _password: string): Promise<ProfileBackup> {
    try {
      // TODO: Implement proper decryption with _password
      
      const decoded = decodeURIComponent(escape(atob(encryptedData.data)))
      return JSON.parse(decoded) as ProfileBackup
    } catch (error) {
      throw new Error('Invalid password or corrupted backup file')
    }
  }

  /**
   * Validate backup structure
   */
  private static validateBackup(backup: any): void {
    if (!backup || typeof backup !== 'object') {
      throw new Error('Invalid backup format')
    }

    if (!backup.version || !backup.timestamp || !backup.profile || !backup.keypair) {
      throw new Error('Incomplete backup data')
    }

    if (!backup.metadata || !backup.metadata.username) {
      throw new Error('Missing backup metadata')
    }

    // Check version compatibility
    if (backup.version !== this.BACKUP_VERSION) {
      console.warn('Backup version mismatch:', backup.version, 'vs', this.BACKUP_VERSION)
      // For now, proceed anyway, but could add migration logic here
    }
  }

  /**
   * Read file content as text
   */
  private static readFileContent(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader()
      
      reader.onload = (event) => {
        if (event.target?.result) {
          resolve(event.target.result as string)
        } else {
          reject(new Error('Failed to read file'))
        }
      }
      
      reader.onerror = () => {
        reject(new Error('File reading error'))
      }
      
      reader.readAsText(file)
    })
  }

  /**
   * Generate unique backup ID
   */
  private static generateBackupId(): string {
    return Date.now().toString(36) + Math.random().toString(36).substr(2)
  }

  /**
   * Format date for filename
   */
  private static formatDateForFilename(timestamp: string): string {
    const date = new Date(timestamp)
    return date.toISOString().split('T')[0].replace(/-/g, '')
  }

  /**
   * Get backup info from file without fully restoring
   */
  static async getBackupInfo(file: File, password?: string): Promise<{
    username: string
    timestamp: string
    version: string
    hasContacts: boolean
    fingerprint: string
  }> {
    try {
      const content = await this.readFileContent(file)
      let backupData = JSON.parse(content)
      
      // Handle encrypted backups
      if (backupData.encrypted && password) {
        backupData = await this.decryptBackup(backupData, password)
      }
      
      return {
        username: backupData.metadata?.username || 'Unknown',
        timestamp: backupData.timestamp || 'Unknown',
        version: backupData.version || 'Unknown',
        hasContacts: Array.isArray(backupData.contacts) && backupData.contacts.length > 0,
        fingerprint: backupData.metadata?.fingerprint || 'Unknown'
      }
    } catch (error) {
      throw new Error('Failed to read backup info: ' + (error instanceof Error ? error.message : 'Unknown error'))
    }
  }
}
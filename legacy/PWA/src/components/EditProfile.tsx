import React, { useState } from 'react'
import { useProfileStore } from '../stores/profileStore'
import { getCore } from '../lib/core'

interface EditProfileProps {
  onCancel: () => void
}

const EditProfile: React.FC<EditProfileProps> = ({ onCancel }) => {
  const { currentProfile, setCurrentProfile } = useProfileStore()
  
  const [formData, setFormData] = useState({
    username: currentProfile?.username || '',
    displayName: currentProfile?.displayName || '',
    bio: currentProfile?.bio || '',
    location: currentProfile?.location || '',
    website: currentProfile?.website || '',
    avatar: currentProfile?.avatar || ''
  })
  
  const [errors, setErrors] = useState<{[key: string]: string}>({})
  const [isLoading, setIsLoading] = useState(false)

  const validateForm = () => {
    const newErrors: {[key: string]: string} = {}
    
    if (formData.website && !formData.website.match(/^https?:\/\/.+/)) {
      newErrors.website = 'Website must be a valid URL (http:// or https://)'
    }
    
    setErrors(newErrors)
    return Object.keys(newErrors).length === 0
  }

  const handleSave = async () => {
    if (!validateForm()) return
    
    setIsLoading(true)
    try {
      const core = await getCore()
      
      // Update core profile data (note: username cannot be changed)
      core.updateProfile(
        formData.displayName,
        formData.bio
      )
      
      // Store extended profile data in localStorage
      const extendedData = {
        location: formData.location,
        website: formData.website,
        avatar: formData.avatar,
        updatedAt: new Date().toISOString()
      }
      localStorage.setItem(`profile-extended-${formData.username}`, JSON.stringify(extendedData))
      
      // Update the store with all data
      const updatedProfile = {
        ...currentProfile!,
        username: formData.username,
        displayName: formData.displayName,
        bio: formData.bio,
        location: formData.location,
        website: formData.website,
        avatar: formData.avatar,
        updatedAt: new Date().toISOString()
      }
      
      setCurrentProfile(updatedProfile)
      onCancel() // Close the edit form
      
    } catch (error) {
      console.error('Failed to update profile:', error)
      setErrors({ general: 'Failed to update profile. Please try again.' })
    } finally {
      setIsLoading(false)
    }
  }

  const handleInputChange = (field: string, value: string) => {
    setFormData(prev => ({ ...prev, [field]: value }))
    // Clear error when user starts typing
    if (errors[field]) {
      setErrors(prev => ({ ...prev, [field]: '' }))
    }
  }

  return (
    <div className="space-y-4 p-4 border rounded-lg bg-white dark:bg-gray-800">
      <h3 className="text-lg font-semibold text-gray-900 dark:text-white">Edit Profile</h3>
      
      {errors.general && (
        <div className="text-red-600 text-sm">{errors.general}</div>
      )}
      
      <div>
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
          Username (cannot be changed)
        </label>
        <input
          type="text"
          value={formData.username}
          readOnly
          className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-gray-100 dark:bg-gray-600 text-gray-600 dark:text-gray-400 cursor-not-allowed"
          placeholder="Enter username"
        />
      </div>
      
      <div>
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
          Display Name
        </label>
        <input
          type="text"
          value={formData.displayName}
          onChange={(e) => handleInputChange('displayName', e.target.value)}
          className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
          placeholder="Enter display name"
        />
      </div>
      
      <div>
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
          Bio
        </label>
        <textarea
          value={formData.bio}
          onChange={(e) => handleInputChange('bio', e.target.value)}
          rows={3}
          className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
          placeholder="Tell us about yourself"
        />
      </div>
      
      <div>
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
          Location
        </label>
        <input
          type="text"
          value={formData.location}
          onChange={(e) => handleInputChange('location', e.target.value)}
          className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
          placeholder="Enter your location"
        />
      </div>
      
      <div>
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
          Website
        </label>
        <input
          type="url"
          value={formData.website}
          onChange={(e) => handleInputChange('website', e.target.value)}
          className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
          placeholder="https://example.com"
        />
        {errors.website && <div className="text-red-600 text-sm mt-1">{errors.website}</div>}
      </div>
      
      <div>
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
          Avatar URL
        </label>
        <input
          type="url"
          value={formData.avatar}
          onChange={(e) => handleInputChange('avatar', e.target.value)}
          className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
          placeholder="https://example.com/avatar.jpg"
        />
      </div>
      
      <div className="flex gap-2 pt-4">
        <button
          onClick={handleSave}
          disabled={isLoading}
          className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-md disabled:opacity-50"
        >
          {isLoading ? 'Saving...' : 'Save Changes'}
        </button>
        <button
          onClick={onCancel}
          disabled={isLoading}
          className="px-4 py-2 bg-gray-300 hover:bg-gray-400 text-gray-700 rounded-md"
        >
          Cancel
        </button>
      </div>
    </div>
  )
}

export default EditProfile
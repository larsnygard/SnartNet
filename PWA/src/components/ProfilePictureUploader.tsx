import React, { useState, useRef, useCallback } from 'react'
import { ImageProcessor, type ProcessedImage } from '../lib/imageProcessor'
import { useProfileStore } from '../stores/profileStore'

interface ProfilePictureUploaderProps {
  onClose?: () => void
  onSuccess?: (imageData: string) => void
}

const ProfilePictureUploader: React.FC<ProfilePictureUploaderProps> = ({ onClose, onSuccess }) => {
  const { currentProfile, updateProfilePicture } = useProfileStore()
  const fileInputRef = useRef<HTMLInputElement>(null)
  
  const [dragActive, setDragActive] = useState(false)
  const [isProcessing, setIsProcessing] = useState(false)
  const [preview, setPreview] = useState<string | null>(null)
  const [processedImage, setProcessedImage] = useState<ProcessedImage | null>(null)
  const [status, setStatus] = useState('')
  const [uploadProgress, setUploadProgress] = useState(0)

  const resetState = () => {
    setPreview(null)
    setProcessedImage(null)
    setStatus('')
    setUploadProgress(0)
    if (fileInputRef.current) {
      fileInputRef.current.value = ''
    }
  }

  const handleFiles = useCallback(async (files: FileList | File[]) => {
    const file = files[0]
    if (!file) return

    try {
      setIsProcessing(true)
      setStatus('📤 Processing image...')
      setUploadProgress(25)

      // Process the image
      const processed = await ImageProcessor.processProfilePicture(file)
      setUploadProgress(75)
      
      // Create thumbnail (validation step)
      await ImageProcessor.createThumbnail(processed)
      setUploadProgress(90)

      setProcessedImage(processed)
      setPreview(processed.dataUrl)
      setStatus(`✅ Image processed successfully! Size: ${ImageProcessor.formatFileSize(processed.size)}`)
      setUploadProgress(100)

      console.log('Image processed:', {
        originalSize: file.size,
        processedSize: processed.size,
        dimensions: `${processed.width}x${processed.height}`,
        format: processed.format
      })

    } catch (error) {
      console.error('Image processing error:', error)
      setStatus(`❌ Error: ${error instanceof Error ? error.message : 'Unknown error'}`)
      resetState()
    } finally {
      setIsProcessing(false)
    }
  }, [])

  const handleFileSelect = (event: React.ChangeEvent<HTMLInputElement>) => {
    const files = event.target.files
    if (files && files.length > 0) {
      handleFiles(files)
    }
  }

  const handleDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    setDragActive(false)

    if (e.dataTransfer.files && e.dataTransfer.files.length > 0) {
      handleFiles(e.dataTransfer.files)
    }
  }, [handleFiles])

  const handleDrag = useCallback((e: React.DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    if (e.type === 'dragenter' || e.type === 'dragover') {
      setDragActive(true)
    } else if (e.type === 'dragleave') {
      setDragActive(false)
    }
  }, [])

  const handleSave = async () => {
    if (!processedImage || !currentProfile) {
      setStatus('❌ No image to save or no current profile')
      return
    }

    try {
      setIsProcessing(true)
      setStatus('💾 Saving profile picture...')

      // Convert to base64 for storage
      const base64 = await ImageProcessor.blobToBase64(processedImage.blob)
      
      // Create thumbnail and convert to base64
      const thumbnail = await ImageProcessor.createThumbnail(processedImage)
      const thumbnailBase64 = await ImageProcessor.blobToBase64(thumbnail.blob)

      // Update profile store
      updateProfilePicture(currentProfile.username, base64, thumbnailBase64)

      setStatus('✅ Profile picture saved successfully!')
      
      // Call success callback
      if (onSuccess) {
        onSuccess(processedImage.dataUrl)
      }

      // Close after delay
      setTimeout(() => {
        if (onClose) {
          onClose()
        }
      }, 1500)

    } catch (error) {
      console.error('Save error:', error)
      setStatus(`❌ Failed to save: ${error instanceof Error ? error.message : 'Unknown error'}`)
    } finally {
      setIsProcessing(false)
    }
  }

  const handleRemove = () => {
    if (!currentProfile) return
    
    if (confirm('Remove your profile picture?')) {
      // Clear profile picture
      updateProfilePicture(currentProfile.username, '', '')
      
      // Remove from localStorage
      localStorage.removeItem(`profile-picture-${currentProfile.username}`)
      localStorage.removeItem(`profile-picture-thumb-${currentProfile.username}`)
      
      setStatus('✅ Profile picture removed')
      
      if (onSuccess) {
        onSuccess('')
      }
      
      setTimeout(() => {
        if (onClose) {
          onClose()
        }
      }, 1000)
    }
  }

  const getProgressWidth = () => {
    if (uploadProgress >= 100) return 'w-full'
    if (uploadProgress >= 90) return 'w-11/12'
    if (uploadProgress >= 75) return 'w-3/4'
    if (uploadProgress >= 50) return 'w-1/2'
    if (uploadProgress >= 25) return 'w-1/4'
    return 'w-0'
  }

  return (
    <div className="max-w-md mx-auto p-6 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg">
      <div className="flex justify-between items-center mb-6">
        <h3 className="text-lg font-semibold text-gray-900 dark:text-white">
          Profile Picture
        </h3>
        {onClose && (
          <button
            onClick={onClose}
            className="text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-300"
          >
            ✕
          </button>
        )}
      </div>

      {/* Upload Area */}
      {!preview && (
        <div
          className={`border-2 border-dashed rounded-lg p-8 text-center transition-colors ${
            dragActive
              ? 'border-blue-400 bg-blue-50 dark:bg-blue-900/20'
              : 'border-gray-300 dark:border-gray-600 hover:border-gray-400 dark:hover:border-gray-500'
          }`}
          onDragEnter={handleDrag}
          onDragLeave={handleDrag}
          onDragOver={handleDrag}
          onDrop={handleDrop}
        >
          <div className="text-4xl mb-4">📷</div>
          <p className="text-gray-600 dark:text-gray-400 mb-4">
            Drag and drop an image here, or click to select
          </p>
          <button
            onClick={() => fileInputRef.current?.click()}
            disabled={isProcessing}
            className="px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-gray-400 text-white rounded-lg transition-colors"
          >
            Choose Image
          </button>
          
          <input
            ref={fileInputRef}
            type="file"
            accept="image/*"
            onChange={handleFileSelect}
            className="hidden"
            aria-label="Select profile picture file"
            title="Select profile picture file"
          />
          
          <div className="mt-4 text-xs text-gray-500 dark:text-gray-400">
            <p>Supported: JPEG, PNG, WebP</p>
            <p>Max size: 2MB • Auto-cropped to square</p>
          </div>
        </div>
      )}

      {/* Preview */}
      {preview && (
        <div className="space-y-4">
          <div className="flex justify-center">
            <div className="relative">
              <img
                src={preview}
                alt="Profile preview"
                className="w-32 h-32 rounded-full object-cover border-4 border-gray-200 dark:border-gray-600"
              />
              {isProcessing && (
                <div className="absolute inset-0 bg-black bg-opacity-50 rounded-full flex items-center justify-center">
                  <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-white"></div>
                </div>
              )}
            </div>
          </div>

          {processedImage && (
            <div className="text-center text-sm text-gray-600 dark:text-gray-400">
              <p>Size: {ImageProcessor.formatFileSize(processedImage.size)}</p>
              <p>Dimensions: {processedImage.width}×{processedImage.height}px</p>
            </div>
          )}

          <div className="flex gap-2">
            <button
              onClick={handleSave}
              disabled={isProcessing || !processedImage}
              className="flex-1 px-4 py-2 bg-green-600 hover:bg-green-700 disabled:bg-gray-400 text-white rounded-lg transition-colors"
            >
              {isProcessing ? '💾 Saving...' : '💾 Save'}
            </button>
            <button
              onClick={resetState}
              disabled={isProcessing}
              className="px-4 py-2 bg-gray-500 hover:bg-gray-600 disabled:bg-gray-400 text-white rounded-lg transition-colors"
            >
              🔄 Try Again
            </button>
          </div>
        </div>
      )}

      {/* Progress Bar */}
      {isProcessing && uploadProgress > 0 && (
        <div className="mt-4">
          <div className="bg-gray-200 dark:bg-gray-700 rounded-full h-2">
            <div
              className={`bg-blue-600 h-2 rounded-full transition-all duration-300 ${getProgressWidth()}`}
            ></div>
          </div>
        </div>
      )}

      {/* Remove existing picture option */}
      {currentProfile?.profilePicture && !preview && (
        <div className="mt-4 pt-4 border-t border-gray-200 dark:border-gray-700">
          <div className="flex items-center justify-center mb-4">
            <img
              src={ImageProcessor.base64ToDataUrl(currentProfile.profilePicture)}
              alt="Current profile"
              className="w-16 h-16 rounded-full object-cover border-2 border-gray-200 dark:border-gray-600"
            />
          </div>
          <button
            onClick={handleRemove}
            className="w-full px-4 py-2 bg-red-600 hover:bg-red-700 text-white rounded-lg transition-colors"
          >
            🗑️ Remove Current Picture
          </button>
        </div>
      )}

      {/* Status Message */}
      {status && (
        <div className={`mt-4 p-3 rounded-lg text-sm ${
          status.startsWith('✅') ? 'bg-green-50 dark:bg-green-900/20 text-green-800 dark:text-green-200' :
          status.startsWith('❌') ? 'bg-red-50 dark:bg-red-900/20 text-red-800 dark:text-red-200' :
          'bg-blue-50 dark:bg-blue-900/20 text-blue-800 dark:text-blue-200'
        }`}>
          {status}
        </div>
      )}
    </div>
  )
}

export default ProfilePictureUploader
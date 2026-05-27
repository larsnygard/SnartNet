import React from 'react'
import { ImageProcessor } from '../lib/imageProcessor'

interface ProfileAvatarProps {
  profilePicture?: string
  username?: string
  size?: 'xs' | 'sm' | 'md' | 'lg' | 'xl'
  className?: string
  showOnlineStatus?: boolean
  isOnline?: boolean
  alt?: string
}

const ProfileAvatar: React.FC<ProfileAvatarProps> = ({
  profilePicture,
  username = 'User',
  size = 'md',
  className = '',
  showOnlineStatus = false,
  isOnline = false,
  alt
}) => {
  const sizeClasses = {
    xs: 'w-6 h-6',
    sm: 'w-8 h-8',
    md: 'w-12 h-12',
    lg: 'w-16 h-16',
    xl: 'w-24 h-24'
  }

  const statusSizeClasses = {
    xs: 'w-1.5 h-1.5 bottom-0 right-0',
    sm: 'w-2 h-2 bottom-0 right-0',
    md: 'w-3 h-3 bottom-0.5 right-0.5',
    lg: 'w-4 h-4 bottom-1 right-1',
    xl: 'w-6 h-6 bottom-1 right-1'
  }

  const getInitials = (name: string) => {
    return name
      .split(' ')
      .map(word => word.charAt(0).toUpperCase())
      .slice(0, 2)
      .join('')
  }

  const avatarContent = profilePicture ? (
    <img
      src={ImageProcessor.base64ToDataUrl(profilePicture)}
      alt={alt || `${username}'s avatar`}
      className={`${sizeClasses[size]} rounded-full object-cover ${className}`}
    />
  ) : (
    <div 
      className={`${sizeClasses[size]} rounded-full bg-gradient-to-br from-blue-400 to-purple-500 flex items-center justify-center text-white font-semibold ${className}`}
      title={username}
    >
      <span className={`${
        size === 'xs' ? 'text-xs' :
        size === 'sm' ? 'text-sm' :
        size === 'md' ? 'text-base' :
        size === 'lg' ? 'text-lg' :
        'text-xl'
      }`}>
        {getInitials(username)}
      </span>
    </div>
  )

  return (
    <div className="relative inline-block">
      {avatarContent}
      
      {/* Online status indicator */}
      {showOnlineStatus && (
        <div
          className={`absolute ${statusSizeClasses[size]} ${
            isOnline ? 'bg-green-400' : 'bg-gray-400'
          } border-2 border-white dark:border-gray-800 rounded-full`}
        />
      )}
    </div>
  )
}

export default ProfileAvatar
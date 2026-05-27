import React, { useState } from 'react';
import { useProfileStore } from '../stores/profileStore';
import ProfileAvatar from './ProfileAvatar';
import ProfilePictureUploader from './ProfilePictureUploader';
import ProfilePosts from './ProfilePosts';

const ProfileDisplay: React.FC = () => {
  const { currentProfile } = useProfileStore();
  const [showUploader, setShowUploader] = useState(false);

  if (!currentProfile) {
    return (
      <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6 text-center">
        <p className="text-gray-700 dark:text-gray-300">No profile loaded.</p>
      </div>
    );
  }

  return (
    <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6 flex flex-col items-center">
      <div className="mb-4 relative">
        <ProfileAvatar
          profilePicture={currentProfile.profilePicture}
          username={currentProfile.username}
          size="xl"
        />
        <button
          className="absolute bottom-0 right-0 bg-blue-600 hover:bg-blue-700 text-white rounded-full p-2 shadow-lg border-2 border-white dark:border-gray-800"
          onClick={() => setShowUploader(true)}
          title="Change profile picture"
        >
          <span role="img" aria-label="Edit">✏️</span>
        </button>
      </div>
      <div className="text-center">
        <h2 className="text-2xl font-bold text-gray-900 dark:text-white mb-1">
          {currentProfile.displayName || currentProfile.username}
        </h2>
        <p className="text-gray-500 dark:text-gray-300 mb-2">@{currentProfile.username}</p>
        {currentProfile.bio && (
          <p className="text-gray-700 dark:text-gray-200 mb-2 max-w-xl mx-auto">{currentProfile.bio}</p>
        )}
      </div>

      {showUploader && (
        <div className="fixed inset-0 bg-black bg-opacity-40 flex items-center justify-center z-50">
          <div className="bg-white dark:bg-gray-900 rounded-lg shadow-lg p-4 relative w-full max-w-lg">
            <button
              className="absolute top-2 right-2 text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
              onClick={() => setShowUploader(false)}
              title="Close"
            >
              ✕
            </button>
            <ProfilePictureUploader onClose={() => setShowUploader(false)} />
          </div>
        </div>
      )}
      {/* Profile posts section */}
      <div className="w-full mt-8">
        <ProfilePosts />
      </div>
    </div>
  );
};

export default ProfileDisplay;


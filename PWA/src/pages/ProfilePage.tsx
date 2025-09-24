import { useProfileStore } from '@/stores/profileStore'
import CreateProfile from '@/components/CreateProfile'
import ProfileDisplay from '@/components/ProfileDisplay'

const ProfilePage: React.FC = () => {
  const { currentProfile, loading } = useProfileStore()

  return (
    <div>
      <h1 className="text-3xl font-bold text-gray-900 dark:text-white mb-6">
        Profile
      </h1>
      
      {loading && (
        <div className="flex items-center justify-center py-8">
          <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-600"></div>
          <span className="ml-3 text-gray-600 dark:text-gray-400">Loading profile...</span>
        </div>
      )}

      {!loading && (
        <div className="space-y-6">
          {currentProfile ? (
            <ProfileDisplay />
          ) : (
            <CreateProfile />
          )}
        </div>
      )}
    </div>
  )
}

export default ProfilePage
import { useEffect, useState } from 'react'
import { useCore } from '@/lib/core'
import CreatePost from '@/components/CreatePost'
import PostTimeline from '@/components/PostTimeline'
import { useProfileStore } from '@/stores/profileStore'

const HomePage: React.FC = () => {
  const { core, loading: coreLoading } = useCore()
  const { currentProfile } = useProfileStore()
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    if (core) {
      setLoading(false)
    }
  }, [core])

  if (coreLoading || loading) {
    return (
      <div className="flex items-center justify-center min-h-64">
        <div className="text-gray-600 dark:text-gray-300">Loading SnartNet...</div>
      </div>
    )
  }

  return (
    <div>
      <h1 className="text-3xl font-bold text-gray-900 dark:text-white mb-4">
        SnartNet
      </h1>
      <p className="text-lg text-gray-600 dark:text-gray-300 mb-8">
        Decentralized social media, powered by swarms.
      </p>
      
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <div className="lg:col-span-2">
          {/* Create Post Section */}
          {currentProfile && <CreatePost />}
          
          {/* Timeline */}
          <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
            <h2 className="text-xl font-semibold mb-6 flex items-center">
              <span>Timeline</span>
              <span className="ml-2 text-sm text-gray-500 dark:text-gray-400">(Torrent-seeded posts)</span>
            </h2>
            <PostTimeline />
          </div>
        </div>
        
        <div className="space-y-6">
          <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
            <h2 className="text-xl font-semibold mb-4">How It Works</h2>
            <p className="text-gray-600 dark:text-gray-300 mb-4">
              Every post is a torrent seed! Your content is distributed across the swarm, 
              making it censorship-resistant and decentralized.
            </p>
            <div className="space-y-2">
              <p className="text-sm text-gray-500">
                🌱 Posts are seeded as torrents
              </p>
              <p className="text-sm text-gray-500">
                📷 Images included in torrent files
              </p>
              <p className="text-sm text-gray-500">
                🔐 Content cryptographically signed
              </p>
              <p className="text-sm text-gray-500">
                🤝 Shared through your Ring of Trust
              </p>
            </div>
          </div>

          <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
            <h3 className="text-lg font-semibold mb-3">Features</h3>
            <div className="space-y-2 text-sm">
              <div className="flex items-center">
                <span className="w-2 h-2 bg-green-500 rounded-full mr-2"></span>
                <span>Profile pictures & posts</span>
              </div>
              <div className="flex items-center">
                <span className="w-2 h-2 bg-green-500 rounded-full mr-2"></span>
                <span>Image upload & processing</span>
              </div>
              <div className="flex items-center">
                <span className="w-2 h-2 bg-green-500 rounded-full mr-2"></span>
                <span>Post torrent seeding</span>
              </div>
              <div className="flex items-center">
                <span className="w-2 h-2 bg-green-500 rounded-full mr-2"></span>
                <span>Chronological timeline</span>
              </div>
              <div className="flex items-center">
                <span className="w-2 h-2 bg-yellow-500 rounded-full mr-2"></span>
                <span>P2P post sharing (WIP)</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

export default HomePage

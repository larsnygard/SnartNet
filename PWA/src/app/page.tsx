import { Link } from 'react-router-dom'
import { useProfileStore } from '@/stores/profileStore'
import Layout from '@/components/Layout'

export default function HomePage() {
  const { currentProfile: profile, loading: isLoading, error } = useProfileStore()

  return (
    <Layout>
      <div className="space-y-8">
        <div>
          <h1 className="text-3xl font-bold text-gray-900 dark:text-white mb-4">
            SnartNet
          </h1>
          <p className="text-lg text-gray-600 dark:text-gray-300 mb-8">
            Decentralized social media, powered by swarms.
          </p>
        </div>
        
        {isLoading && (
          <div className="flex items-center justify-center py-12">
            <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-600 mr-3"></div>
            <p className="text-gray-600 dark:text-gray-400">Initializing WASM core...</p>
          </div>
        )}

        {error && (
          <div className="bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg p-6">
            <h2 className="text-xl font-semibold text-red-800 dark:text-red-200 mb-2">
              Initialization Error
            </h2>
            <p className="text-red-700 dark:text-red-300">
              {error}
            </p>
          </div>
        )}
        
        {!isLoading && !error && (
          <>
            {profile ? (
              <div className="bg-gradient-to-r from-green-50 to-blue-50 dark:from-green-900/20 dark:to-blue-900/20 rounded-lg p-6">
                <h2 className="text-xl font-semibold text-gray-900 dark:text-white mb-2">
                  Welcome back, {profile.displayName || profile.username}! 🎉
                </h2>
                <p className="text-gray-600 dark:text-gray-400 mb-4">
                  Your cryptographic identity is active and ready to interact with the decentralized network.
                </p>
                <div className="flex flex-wrap gap-3">
                  <Link 
                    to="/profile" 
                    className="inline-flex items-center px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors"
                  >
                    Manage Profile
                  </Link>
                  <Link 
                    to="/messages" 
                    className="inline-flex items-center px-4 py-2 bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 rounded-lg hover:bg-gray-50 dark:hover:bg-gray-700 text-gray-700 dark:text-gray-300 transition-colors"
                  >
                    Messages
                  </Link>
                </div>
              </div>
            ) : (
              <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
                <h2 className="text-xl font-semibold mb-4">Getting Started</h2>
                <p className="text-gray-600 dark:text-gray-300 mb-4">
                  Welcome to SnartNet! This is a decentralized social media platform built on 
                  BitTorrent-like swarm technology and modern cryptography.
                </p>
                <div className="space-y-2 mb-6">
                  <p className="text-sm text-gray-500">
                    🔐 Your identity is cryptographically secured
                  </p>
                  <p className="text-sm text-gray-500">
                    🌐 Content is distributed peer-to-peer via magnet links
                  </p>
                  <p className="text-sm text-gray-500">
                    🤝 Trust is managed through your Ring of Trust
                  </p>
                </div>
                
                <div className="p-4 bg-blue-50 dark:bg-blue-900/20 rounded-lg">
                  <p className="text-blue-700 dark:text-blue-300 mb-3">
                    🚀 Ready to get started! Create your cryptographic identity to join the network.
                  </p>
                  <Link
                    to="/profile"
                    className="inline-flex items-center px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors"
                  >
                    Create Profile
                  </Link>
                </div>
              </div>
            )}
          </>
        )}
      </div>
    </Layout>
  )
}

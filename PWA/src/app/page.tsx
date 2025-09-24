import { Link } from 'react-router-dom'
import { useProfileStore } from '@/stores/profileStore'
import Layout from '@/components/Layout'

export default function HomePage() {
  const { currentProfile: profile, loading: isLoading, error } = useProfileStore()

  return (
    <Layout>
      <div>
        <h1 className="text-3xl font-bold text-gray-900 dark:text-white mb-4">
          SnartNet
        </h1>
        <p className="text-lg text-gray-600 dark:text-gray-300 mb-8">
          Decentralized social media, powered by swarms.
        </p>
        
        <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
          <h2 className="text-xl font-semibold mb-4">Getting Started</h2>
          <p className="text-gray-600 dark:text-gray-300 mb-4">
            Welcome to SnartNet! This is a decentralized social media platform built on 
            BitTorrent-like swarm technology and modern cryptography.
          </p>
          <div className="space-y-2">
            <p className="text-sm text-gray-500">
              🔐 Your identity is cryptographically secured
            </p>
            <p className="text-sm text-gray-500">
              🌐 Content is distributed peer-to-peer
            </p>
            <p className="text-sm text-gray-500">
              🤝 Trust is managed through your Ring of Trust
            </p>
          </div>
          
          {isLoading && (
            <div className="mt-4 flex items-center text-blue-600">
              <div className="animate-spin rounded-full h-4 w-4 border-b-2 border-blue-600 mr-2"></div>
              <p>Initializing WASM core...</p>
            </div>
          )}

          {error && (
            <div className="mt-4 p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg">
              <p className="text-red-700 dark:text-red-300 text-sm">
                Error: {error}
              </p>
            </div>
          )}
          
          {!isLoading && !error && (
            <>
              {profile ? (
                <div className="mt-4 p-4 bg-green-50 dark:bg-green-900/20 rounded-lg">
                  <p className="text-green-700 dark:text-green-300 mb-2">
                    Welcome back, {profile.displayName || profile.username}! 🎉
                  </p>
                  <div className="flex space-x-3">
                    <Link 
                      to="/profile" 
                      className="text-green-700 dark:text-green-300 underline hover:text-green-800 dark:hover:text-green-200"
                    >
                      View Profile
                    </Link>
                    <span className="text-green-600">•</span>
                    <Link 
                      to="/messages" 
                      className="text-green-700 dark:text-green-300 underline hover:text-green-800 dark:hover:text-green-200"
                    >
                      Messages
                    </Link>
                  </div>
                </div>
              ) : (
                <div className="mt-4 p-4 bg-blue-50 dark:bg-blue-900/20 rounded-lg">
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
              )}
            </>
          )}
        </div>
      </div>
    </Layout>
  )
}

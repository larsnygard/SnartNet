import { useEffect, useState } from 'react'
import { useCore } from '@/lib/core'

const HomePage: React.FC = () => {
  const { core, loading: coreLoading } = useCore()
  const [timeline, setTimeline] = useState<any[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    if (core) {
      core.getTimeline().then(posts => {
        setTimeline(posts)
        setLoading(false)
      })
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
          <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6 mb-6">
            <h2 className="text-xl font-semibold mb-4">Timeline</h2>
            <div className="space-y-4">
              {timeline.map(post => (
                <div key={post.id} className="border-b border-gray-200 dark:border-gray-700 pb-4 last:border-0">
                  <div className="flex items-center justify-between mb-2">
                    <span className="font-medium text-gray-900 dark:text-white">@{post.author}</span>
                    <span className="text-sm text-gray-500">{new Date(post.timestamp).toLocaleString()}</span>
                  </div>
                  <p className="text-gray-700 dark:text-gray-300 mb-2">{post.content}</p>
                  {post.tags && post.tags.length > 0 && (
                    <div className="flex gap-2">
                      {post.tags.map((tag: string) => (
                        <span key={tag} className="text-xs bg-blue-100 dark:bg-blue-900 text-blue-800 dark:text-blue-200 px-2 py-1 rounded">
                          #{tag}
                        </span>
                      ))}
                    </div>
                  )}
                </div>
              ))}
            </div>
          </div>
        </div>
        
        <div className="space-y-6">
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
          </div>

          <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
            <h3 className="text-lg font-semibold mb-3">Development Status</h3>
            <div className="space-y-2 text-sm">
              <div className="flex items-center">
                <span className="w-2 h-2 bg-green-500 rounded-full mr-2"></span>
                <span>React PWA setup complete</span>
              </div>
              <div className="flex items-center">
                <span className="w-2 h-2 bg-green-500 rounded-full mr-2"></span>
                <span>Mock core interface active</span>
              </div>
              <div className="flex items-center">
                <span className="w-2 h-2 bg-yellow-500 rounded-full mr-2"></span>
                <span>Rust WASM core (planned)</span>
              </div>
              <div className="flex items-center">
                <span className="w-2 h-2 bg-gray-400 rounded-full mr-2"></span>
                <span>P2P networking (future)</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

export default HomePage

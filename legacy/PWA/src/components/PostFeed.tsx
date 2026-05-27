import { useState, useEffect, useMemo } from 'react'
import { usePostStore } from '@/stores/postStore'

type TimePeriod = 'all' | '24h' | '7d' | '30d';

export default function PostFeed() {
  const posts = usePostStore((state) => state.posts)
  const loading = usePostStore((state) => state.loading)
  const error = usePostStore((state) => state.error)
  const loadPosts = usePostStore((state) => state.loadPostsFromContacts)

  const [postCount, setPostCount] = useState(20)
  const [timePeriod, setTimePeriod] = useState<TimePeriod>('all')

  useEffect(() => {
    loadPosts()
  }, [loadPosts])

  const filteredPosts = useMemo(() => {
    let filtered = posts

    if (timePeriod !== 'all') {
      const now = new Date().getTime()
      const periodMs = {
        '24h': 24 * 60 * 60 * 1000,
        '7d': 7 * 24 * 60 * 60 * 1000,
        '30d': 30 * 24 * 60 * 60 * 1000,
      }[timePeriod]

      filtered = filtered.filter(post => {
        const postDate = new Date(post.createdAt).getTime()
        return now - postDate <= periodMs
      })
    }

    return filtered.slice(0, postCount)
  }, [posts, postCount, timePeriod])

  const formatTimestamp = (timestamp: string) => {
    try {
      const date = new Date(timestamp)
      const now = new Date()
      const diffMs = now.getTime() - date.getTime()
      const diffMins = Math.floor(diffMs / (1000 * 60))
      const diffHours = Math.floor(diffMs / (1000 * 60 * 60))
      const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24))

      if (diffMins < 1) return 'Just now'
      if (diffMins < 60) return `${diffMins}m ago`
      if (diffHours < 24) return `${diffHours}h ago`
      if (diffDays < 7) return `${diffDays}d ago`
      
      return date.toLocaleDateString()
    } catch {
      return 'Unknown time'
    }
  }

  const copySignature = async (signature: string) => {
    try {
      await navigator.clipboard.writeText(signature)
      alert('Signature copied to clipboard!')
    } catch (error) {
      console.error('Failed to copy signature:', error)
    }
  }

  if (loading) {
    return (
      <div className="space-y-4">
        {[1, 2, 3].map(i => (
          <div key={i} className="bg-white dark:bg-gray-800 rounded-lg shadow p-6 animate-pulse">
            <div className="flex items-center space-x-3 mb-3">
              <div className="w-8 h-8 bg-gray-200 dark:bg-gray-700 rounded-full"></div>
              <div className="flex-1">
                <div className="h-4 bg-gray-200 dark:bg-gray-700 rounded w-24 mb-1"></div>
                <div className="h-3 bg-gray-200 dark:bg-gray-700 rounded w-16"></div>
              </div>
            </div>
            <div className="space-y-2">
              <div className="h-4 bg-gray-200 dark:bg-gray-700 rounded w-full"></div>
              <div className="h-4 bg-gray-200 dark:bg-gray-700 rounded w-3/4"></div>
            </div>
          </div>
        ))}
      </div>
    )
  }

  if (error) {
    return (
      <div className="bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg p-4">
        <p className="text-red-700 dark:text-red-300">
          Error loading posts: {error}
        </p>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-col sm:flex-row justify-between items-start sm:items-center gap-4">
        <h2 className="text-xl font-bold text-gray-900 dark:text-white">
          Recent Posts ({filteredPosts.length} of {posts.length})
        </h2>
        <div className="flex items-center space-x-2 text-sm">
          <label htmlFor="timePeriodSelect" className="sr-only">Filter by time</label>
          <select
            id="timePeriodSelect"
            value={timePeriod}
            onChange={(e) => setTimePeriod(e.target.value as TimePeriod)}
            className="bg-white dark:bg-gray-700 border border-gray-300 dark:border-gray-600 rounded-md px-2 py-1"
          >
            <option value="all">All Time</option>
            <option value="24h">Last 24h</option>
            <option value="7d">Last 7d</option>
            <option value="30d">Last 30d</option>
          </select>
          <label htmlFor="postCountSelect" className="sr-only">Number of posts to show</label>
          <select
            id="postCountSelect"
            value={postCount}
            onChange={(e) => setPostCount(Number(e.target.value))}
            className="bg-white dark:bg-gray-700 border border-gray-300 dark:border-gray-600 rounded-md px-2 py-1"
          >
            <option value={10}>Show 10</option>
            <option value={20}>Show 20</option>
            <option value={50}>Show 50</option>
            <option value={100}>Show 100</option>
          </select>
        </div>
      </div>

      {filteredPosts.length === 0 ? (
        <div className="bg-gray-50 dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg p-8 text-center">
          <div className="text-gray-400 dark:text-gray-500 mb-4">
            <svg className="w-16 h-16 mx-auto" fill="currentColor" viewBox="0 0 20 20">
              <path fillRule="evenodd" d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7-4a1 1 0 11-2 0 1 1 0 012 0zM9 9a1 1 0 000 2v3a1 1 0 001 1h1a1 1 0 100-2v-3a1 1 0 00-1-1H9z" clipRule="evenodd" />
            </svg>
          </div>
          <h3 className="text-lg font-medium text-gray-900 dark:text-white mb-2">
            No posts found
          </h3>
          <p className="text-gray-600 dark:text-gray-400">
            Try adjusting your filters or wait for new posts from the network.
          </p>
        </div>
      ) : (
        <div className="space-y-4">
          {filteredPosts.map((post) => (
            <article key={post.id} className="bg-white dark:bg-gray-800 rounded-lg shadow border border-gray-200 dark:border-gray-700 p-6">
              <div className="flex items-center space-x-3 mb-4">
                <div className="w-8 h-8 bg-gradient-to-br from-blue-500 to-purple-600 rounded-full flex items-center justify-center text-white text-sm font-medium">
                  {post.author.charAt(0).toUpperCase()}
                </div>
                <div className="flex-1">
                  <div className="flex items-center flex-wrap gap-2">
                    <span className="font-medium text-gray-900 dark:text-white">
                      {post.authorDisplayName || post.author}
                    </span>
                    {post.isSeeding && (
                      <span className="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium bg-blue-100 dark:bg-blue-900/40 text-blue-700 dark:text-blue-300">
                        <svg className="w-3 h-3 mr-1 animate-spin" viewBox="0 0 24 24" fill="none"><circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"/><path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v4a4 4 0 00-4 4H4z"/></svg>
                        Seeding...
                      </span>
                    )}
                    {post.seedError && (
                      <span className="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium bg-red-100 dark:bg-red-900/40 text-red-700 dark:text-red-300" title={post.seedError}>
                        <svg className="w-3 h-3 mr-1" viewBox="0 0 20 20" fill="currentColor"><path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM9 6a1 1 0 012 0v4a1 1 0 01-2 0V6zm1 8a1.25 1.25 0 110-2.5A1.25 1.25 0 0110 14z" clipRule="evenodd" /></svg>
                        Seed Failed
                      </span>
                    )}
                    {!post.isSeeding && !post.seedError && post.magnetUri && (
                      <span className="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium bg-green-100 dark:bg-green-900/40 text-green-700 dark:text-green-300">
                        <svg className="w-3 h-3 mr-1" viewBox="0 0 20 20" fill="currentColor"><path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clipRule="evenodd"/></svg>
                        Seeded
                      </span>
                    )}
                    {post.magnetUri && (
                      <span className="inline-flex items-center text-xs text-green-600 dark:text-green-400">
                        <svg className="w-3 h-3 mr-1" fill="currentColor" viewBox="0 0 20 20">
                          <path fillRule="evenodd" d="M6.267 3.455a3.066 3.066 0 001.745-.723 3.066 3.066 0 013.976 0 3.066 3.066 0 001.745.723 3.066 3.066 0 012.812 2.812c.051.643.304 1.254.723 1.745a3.066 3.066 0 010 3.976 3.066 3.066 0 00-.723 1.745 3.066 3.066 0 01-2.812 2.812 3.066 3.066 0 00-1.745.723 3.066 3.066 0 01-3.976 0 3.066 3.066 0 00-1.745-.723 3.066 3.066 0 01-2.812-2.812 3.066 3.066 0 00-.723-1.745 3.066 3.066 0 010-3.976 3.066 3.066 0 00.723-1.745 3.066 3.066 0 012.812-2.812zm7.44 5.252a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clipRule="evenodd" />
                        </svg>
                        Verified
                      </span>
                    )}
                  </div>
                  <div className="flex items-center gap-2 text-sm text-gray-500 dark:text-gray-400">
                    <span>{formatTimestamp(post.createdAt)}</span>
                    {post.isSeeding && (
                      <span className="flex items-center gap-1 text-blue-600 dark:text-blue-400 text-xs">
                        {Math.round(post.seedProgress || 0)}%
                        <span className="w-16 h-1 bg-gray-200 dark:bg-gray-700 rounded overflow-hidden" aria-label="Seeding progress">
                          {(() => {
                            const pct = Math.max(0, Math.min(100, Math.round(post.seedProgress || 0)))
                            const buckets = [0,5,10,15,20,25,30,35,40,45,50,55,60,65,70,75,80,85,90,95,100]
                            const bucket = buckets.find(b => pct <= b) || 100
                            const widthClass = `w-[${bucket}%]`
                            return <span className={`block h-full bg-blue-500 dark:bg-blue-400 transition-all ${widthClass}`} data-progress={pct}></span>
                          })()}
                        </span>
                      </span>
                    )}
                  </div>
                </div>
              </div>

              <div className="mb-4">
                <p className="text-gray-900 dark:text-white whitespace-pre-wrap">
                  {post.content}
                </p>
              </div>

              {post.images && post.images.length > 0 && (
                <div className="grid grid-cols-2 sm:grid-cols-3 gap-2 mb-4">
                  {post.images.map(image => (
                    <img key={image.id} src={image.data} alt="Post attachment" className="w-full h-auto object-cover rounded-md" />
                  ))}
                </div>
              )}

              {post.tags && post.tags.length > 0 && (
                <div className="flex flex-wrap gap-2 mb-4">
                  {post.tags.map((tag, index) => (
                    <span
                      key={index}
                      className="inline-flex items-center px-2 py-1 rounded-full text-xs font-medium bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300"
                    >
                      #{tag}
                    </span>
                  ))}
                </div>
              )}

              {post.magnetUri && (
                <div className="border-t border-gray-200 dark:border-gray-700 pt-4">
                  <details className="group">
                    <summary className="cursor-pointer text-sm text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white">
                      🌱 Torrent Info
                      <svg className="inline w-4 h-4 ml-1 transform group-open:rotate-180 transition-transform" fill="currentColor" viewBox="0 0 20 20">
                        <path fillRule="evenodd" d="M5.293 7.293a1 1 0 011.414 0L10 10.586l3.293-3.293a1 1 0 111.414 1.414l-4 4a1 1 0 01-1.414 0l-4-4a1 1 0 010-1.414z" clipRule="evenodd" />
                      </svg>
                    </summary>
                    <div className="mt-2 space-y-2">
                      <div className="text-xs text-gray-500 dark:text-gray-400">
                        This post is being seeded as a torrent.
                      </div>
                      <div className="flex items-center space-x-2">
                        <code className="text-xs bg-gray-100 dark:bg-gray-700 px-2 py-1 rounded font-mono text-gray-800 dark:text-gray-200 flex-1 truncate">
                          {post.magnetUri}
                        </code>
                        <button
                          onClick={() => copySignature(post.magnetUri!)}
                          className="text-blue-600 hover:text-blue-700 dark:text-blue-400 dark:hover:text-blue-300 text-xs"
                        >
                          Copy Magnet
                        </button>
                      </div>
                    </div>
                  </details>
                </div>
              )}
            </article>
          ))}
        </div>
      )}
    </div>
  )
}
import React, { useEffect, useMemo } from 'react'
import { usePostStore, TorrentPost } from '../stores/postStore'
import { useContactStore } from '../stores/contactStore'
import { useProfileStore } from '@/stores/profileStore'
import { ImageProcessor } from '../lib/imageProcessor'

const PostTimeline: React.FC = () => {
  const { posts, loading, loadPostsFromContacts } = usePostStore()
  const { loadContacts, contacts } = useContactStore()
  const { currentProfile } = useProfileStore()

  useEffect(() => {
    // Load contacts and their posts
    loadContacts()
    loadPostsFromContacts()
  }, [loadContacts, loadPostsFromContacts])



  // Compute allowed author identifiers (username or public key fingerprint basis)
  const allowedAuthors = useMemo(() => {
    const set = new Set<string>()
    if (currentProfile) {
      if (currentProfile.publicKey) set.add(currentProfile.publicKey)
      set.add(currentProfile.username)
    }
    contacts.forEach(c => {
      if (c.relationship === 'friend' || c.relationship === 'ring-of-trust') {
        set.add(c.username)
        // In future we may map to public key fingerprint
      }
    })
    return set
  }, [contacts, currentProfile])

  const filteredPosts = useMemo(() => {
    // If no contacts yet, show all local posts (author = current profile) so user sees own posts
    if (allowedAuthors.size === 0) return posts
    return posts.filter(p => allowedAuthors.has(p.author) || (p.authorDisplayName && allowedAuthors.has(p.authorDisplayName)))
  }, [posts, allowedAuthors])

  if (loading) {
    return (
      <div className="flex items-center justify-center py-8">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-600"></div>
        <span className="ml-3 text-gray-600 dark:text-gray-400">Loading timeline...</span>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      {filteredPosts.length === 0 ? (
        <div className="text-center py-12">
          <div className="text-6xl mb-4">🌱</div>
          <h3 className="text-lg font-medium text-gray-900 dark:text-white mb-2">
            No posts yet
          </h3>
          <p className="text-gray-500 dark:text-gray-400">
            Add some friends or seed a post to see activity here.
          </p>
        </div>
      ) : (
        filteredPosts.map((post: TorrentPost) => (
          <PostCard key={post.id} post={post} />
        ))
      )}
    </div>
  )
}

interface PostCardProps {
  post: TorrentPost
}

const PostCard: React.FC<PostCardProps> = ({ post }) => {
  // Show a clear placeholder if the post is unavailable
  if (post.content?.startsWith('(Unavailable content') || post.content?.startsWith('(Failed to fetch')) {
    return (
      <div className="bg-white dark:bg-gray-800 rounded-lg shadow border border-gray-200 dark:border-gray-700 p-6 text-center py-8 text-gray-400 dark:text-gray-500">
        <div className="text-4xl mb-2">🕸️</div>
        <div className="font-semibold">Post unavailable</div>
        <div className="text-xs mt-1">Missing seeder: <span className="font-mono">{post.authorDisplayName || post.author}</span></div>
        <div className="text-xs mt-1">Magnet: <span className="break-all font-mono">{post.magnetUri}</span></div>
      </div>
    )
  }
  // ...existing code for rendering a real post...
  return (
    <div className="bg-white dark:bg-gray-800 rounded-lg shadow border border-gray-200 dark:border-gray-700 p-6">
      {/* Post header */}
      <div className="flex items-center space-x-3 mb-4">
        <div className="w-10 h-10 rounded-full bg-gradient-to-br from-blue-400 to-purple-500 flex items-center justify-center text-white font-semibold">
          {post.authorAvatar ? (
            <img 
              src={ImageProcessor.base64ToDataUrl(post.authorAvatar)}
              alt={post.author}
              className="w-10 h-10 rounded-full object-cover"
            />
          ) : (
            post.author.charAt(0).toUpperCase()
          )}
        </div>
        <div className="flex-1">
          <div className="flex items-center space-x-2">
            <h3 className="font-semibold text-gray-900 dark:text-white">
              {post.authorDisplayName || post.author}
            </h3>
            <span className="text-gray-500 dark:text-gray-400">@{post.author}</span>
            {post.signatureVerified ? (
              <span
                className="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-semibold bg-green-100 dark:bg-green-900 text-green-700 dark:text-green-300 border border-green-300 dark:border-green-700"
                title={`Signature verified${post.fingerprint ? ` • ${post.fingerprint}` : ''}`}
              >
                ✅ ver
              </span>
            ) : (
              <span
                className="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-semibold bg-yellow-100 dark:bg-yellow-900 text-yellow-700 dark:text-yellow-300 border border-yellow-300 dark:border-yellow-700"
                title={post.signatureError === 'missing-signature' ? 'No signature present' : `Signature invalid: ${post.signatureError || 'unknown'}`}
              >
                ⚠️ unverified
              </span>
            )}
          </div>
          <div className="flex items-center space-x-2 text-sm text-gray-500 dark:text-gray-400">
            <span>{formatTimeAgo(post.createdAt)}</span>
            {post.isSeeding && (
              <span className="flex items-center text-green-600 dark:text-green-400">
                <span className="w-2 h-2 bg-green-500 rounded-full mr-1"></span>
                Seeding
              </span>
            )}
            {post.magnetUri && (
              <span className="flex items-center text-blue-600 dark:text-blue-400">
                🧲 Torrent
              </span>
            )}
          </div>
        </div>
      </div>

      {/* Post content */}
      {post.content && (
        <div className="mb-4">
          <p className="text-gray-900 dark:text-gray-100 whitespace-pre-line">
            {post.content}
          </p>
        </div>
      )}

      {/* Post images */}
      {post.images && post.images.length > 0 && (
        <div className={`mb-4 grid gap-2 ${
          post.images.length === 1 ? 'grid-cols-1' :
          post.images.length === 2 ? 'grid-cols-2' :
          post.images.length === 3 ? 'grid-cols-2' :
          'grid-cols-2'
        }`}>
          {post.images.map((image, index) => (
            <img
              key={index}
              src={ImageProcessor.base64ToDataUrl(image.data)}
              alt={image.filename}
              className="rounded object-cover w-full"
            />
          ))}
        </div>
      )}

      {/* Magnet link copy button */}
      {post.magnetUri && (
        <button 
          onClick={() => navigator.clipboard.writeText(post.magnetUri!)}
          className="text-xs text-blue-600 hover:text-blue-700 dark:text-blue-400 dark:hover:text-blue-300"
          title="Copy magnet link"
        >
          🧲 Copy Magnet
        </button>
      )}
    </div>
  )
}

const formatTimeAgo = (dateString: string) => {
  const date = new Date(dateString)
  const now = new Date()
  const diffInSeconds = Math.floor((now.getTime() - date.getTime()) / 1000)

  if (diffInSeconds < 60) return 'just now'
  if (diffInSeconds < 3600) return `${Math.floor(diffInSeconds / 60)}m ago`
  if (diffInSeconds < 86400) return `${Math.floor(diffInSeconds / 3600)}h ago`
  if (diffInSeconds < 604800) return `${Math.floor(diffInSeconds / 86400)}d ago`
  
  return date.toLocaleDateString()
}

export default PostTimeline
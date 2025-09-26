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
            <div 
              key={image.id} 
              className={`${
                post.images!.length === 3 && index === 0 ? 'col-span-2' : ''
              }`}
            >
              <img
                src={ImageProcessor.base64ToDataUrl(image.data)}
                alt={image.filename}
                className="w-full h-64 object-cover rounded-lg border border-gray-200 dark:border-gray-600 cursor-pointer hover:opacity-90 transition-opacity"
                onClick={() => {
                  // TODO: Open image in modal/lightbox
                  window.open(ImageProcessor.base64ToDataUrl(image.data), '_blank')
                }}
              />
            </div>
          ))}
        </div>
      )}

      {/* Post tags */}
      {post.tags && post.tags.length > 0 && (
        <div className="mb-4">
          <div className="flex flex-wrap gap-2">
            {post.tags.map((tag) => (
              <span
                key={tag}
                className="px-2 py-1 text-xs bg-blue-100 dark:bg-blue-900 text-blue-800 dark:text-blue-200 rounded-full"
              >
                #{tag}
              </span>
            ))}
          </div>
        </div>
      )}

      {/* Post actions */}
      <div className="flex items-center justify-between pt-4 border-t border-gray-200 dark:border-gray-700">
        <div className="flex items-center space-x-4">
          <button className="flex items-center space-x-1 text-gray-500 hover:text-blue-600 dark:text-gray-400 dark:hover:text-blue-400 transition-colors">
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4.318 6.318a4.5 4.5 0 000 6.364L12 20.364l7.682-7.682a4.5 4.5 0 00-6.364-6.364L12 7.636l-1.318-1.318a4.5 4.5 0 00-6.364 0z" />
            </svg>
            <span className="text-sm">Like</span>
          </button>
          
          <button className="flex items-center space-x-1 text-gray-500 hover:text-blue-600 dark:text-gray-400 dark:hover:text-blue-400 transition-colors">
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
            </svg>
            <span className="text-sm">Reply</span>
          </button>

          <button className="flex items-center space-x-1 text-gray-500 hover:text-blue-600 dark:text-gray-400 dark:hover:text-blue-400 transition-colors">
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8.684 13.342C8.886 12.938 9 12.482 9 12c0-.482-.114-.938-.316-1.342m0 2.684a3 3 0 110-2.684m0 2.684l6.632 3.316m-6.632-6l6.632-3.316m0 0a3 3 0 105.367-2.684 3 3 0 00-5.367 2.684zm0 9.316a3 3 0 105.367 2.684 3 3 0 00-5.367-2.684z" />
            </svg>
            <span className="text-sm">Share</span>
          </button>
        </div>

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
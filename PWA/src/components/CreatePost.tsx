import { useState } from 'react'
import { getCore } from '@/lib/core'
import { useProfileStore } from '@/stores/profileStore'

export default function CreatePost() {
  const [content, setContent] = useState('')
  const [tags, setTags] = useState('')
  const [isLoading, setIsLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState<string | null>(null)

  const { currentProfile } = useProfileStore()

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    
    if (!content.trim()) {
      setError('Post content is required')
      return
    }

    if (!currentProfile) {
      setError('You need to create a profile first')
      return
    }

    setIsLoading(true)
    setError(null)
    setSuccess(null)

    try {
      console.log('[CreatePost] Creating post...')
      
      // Get the core instance
      const core = await getCore()

      // Parse tags
      const tagList = tags.trim() 
        ? tags.split(',').map(tag => tag.trim()).filter(tag => tag.length > 0)
        : undefined

      // Create signed post using WASM core
      const signedPost = await core.createPost(
        content.trim(),
        tagList
      )
      
      console.log('[CreatePost] Post created and signed:', signedPost)
      setSuccess('Post created and cryptographically signed! 🎉')
      
      // Clear form
      setContent('')
      setTags('')
      
    } catch (err) {
      console.error('[CreatePost] Error creating post:', err)
      const errorMessage = err instanceof Error ? err.message : 'Failed to create post'
      setError(errorMessage)
    } finally {
      setIsLoading(false)
    }
  }

  if (!currentProfile) {
    return (
      <div className="bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-800 rounded-lg p-4">
        <p className="text-yellow-700 dark:text-yellow-300">
          You need to create a profile before you can make posts.
        </p>
      </div>
    )
  }

  return (
    <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
      <h2 className="text-xl font-bold text-gray-900 dark:text-white mb-4">
        Create New Post
      </h2>
      
      <form onSubmit={handleSubmit} className="space-y-4">
        <div>
          <label htmlFor="content" className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
            What's on your mind? *
          </label>
          <textarea
            id="content"
            value={content}
            onChange={(e) => setContent(e.target.value)}
            rows={4}
            className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md shadow-sm bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent resize-none"
            placeholder="Share your thoughts with the decentralized network..."
            disabled={isLoading}
            required
          />
          <div className="mt-1 text-right text-sm text-gray-500">
            {content.length} characters
          </div>
        </div>

        <div>
          <label htmlFor="tags" className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
            Tags (optional)
          </label>
          <input
            type="text"
            id="tags"
            value={tags}
            onChange={(e) => setTags(e.target.value)}
            className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md shadow-sm bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent"
            placeholder="crypto, decentralized, snartnet (comma-separated)"
            disabled={isLoading}
          />
          <div className="mt-1 text-sm text-gray-500">
            Separate tags with commas
          </div>
        </div>

        {error && (
          <div className="p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-md">
            <p className="text-red-700 dark:text-red-300 text-sm">{error}</p>
          </div>
        )}

        {success && (
          <div className="p-3 bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800 rounded-md">
            <p className="text-green-700 dark:text-green-300 text-sm">{success}</p>
          </div>
        )}

        <div className="flex items-center justify-between pt-4 border-t border-gray-200 dark:border-gray-700">
          <div className="flex items-center text-sm text-gray-500">
            <svg className="w-4 h-4 mr-1" fill="currentColor" viewBox="0 0 20 20">
              <path fillRule="evenodd" d="M5 9V7a5 5 0 0110 0v2a2 2 0 012 2v5a2 2 0 01-2 2H5a2 2 0 01-2-2v-5a2 2 0 012-2zm8-2v2H7V7a3 3 0 016 0z" clipRule="evenodd" />
            </svg>
            Cryptographically signed as {currentProfile.username}
          </div>
          
          <button
            type="submit"
            disabled={isLoading || !content.trim()}
            className="bg-blue-600 hover:bg-blue-700 disabled:bg-gray-400 text-white font-medium py-2 px-6 rounded-lg transition-colors disabled:cursor-not-allowed"
          >
            {isLoading ? (
              <span className="flex items-center">
                <svg className="animate-spin -ml-1 mr-2 h-4 w-4 text-white" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                  <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                  <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                </svg>
                Publishing...
              </span>
            ) : (
              'Publish Post'
            )}
          </button>
        </div>
      </form>

      <div className="mt-6 p-4 bg-gray-50 dark:bg-gray-700/50 rounded-lg">
        <h3 className="text-sm font-medium text-gray-900 dark:text-white mb-2">
          🔐 How Post Signing Works
        </h3>
        <ul className="text-xs text-gray-600 dark:text-gray-400 space-y-1">
          <li>• Your post content is cryptographically signed with your Ed25519 private key</li>
          <li>• Anyone can verify the post came from you using your public key</li>
          <li>• The signature proves authenticity and prevents tampering</li>
          <li>• Posts are timestamped and ready for P2P distribution</li>
        </ul>
      </div>
    </div>
  )
}
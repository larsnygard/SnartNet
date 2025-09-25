import { useState } from 'react'
import { useProfileStore } from '@/stores/profileStore'

export default function CreatePost() {
  const [content, setContent] = useState('')
  const [isPosting, setIsPosting] = useState(false)
  const { currentProfile } = useProfileStore()

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!content.trim() || !currentProfile) return

    setIsPosting(true)
    setTimeout(() => {
      console.log('Post created:', content)
      setContent('')
      setIsPosting(false)
    }, 1000)
  }

  if (!currentProfile) {
    return (
      <div className="bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-800 rounded-lg p-4 mb-6">
        <p className="text-yellow-700 dark:text-yellow-300">
          You need to create a profile before you can make posts.
        </p>
      </div>
    )
  }

  return (
    <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6 mb-6">
      <h2 className="text-xl font-bold text-gray-900 dark:text-white mb-4">
        Create New Post
      </h2>
      
      <form onSubmit={handleSubmit} className="space-y-4">
        <textarea
          value={content}
          onChange={(e) => setContent(e.target.value)}
          rows={4}
          className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md shadow-sm bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent resize-none"
          placeholder="What's happening on the swarm?"
          disabled={isPosting}
        />
        
        <div className="flex justify-between items-center">
          <span className="text-sm text-gray-500">Posts are seeded as torrents</span>
          <button
            type="submit"
            disabled={isPosting || !content.trim()}
            className="px-6 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-gray-400 text-white font-medium rounded-lg transition-colors"
          >
            {isPosting ? '🌱 Seeding...' : '🌱 Seed Post'}
          </button>
        </div>
      </form>
    </div>
  )
}

import { useState, useRef } from 'react'
import { useProfileStore } from '@/stores/profileStore'
import { usePostStore, PostImage } from '@/stores/postStore'
import { processAndStoreImage } from '@/lib/imageProcessor'

export default function CreatePost() {
  const [content, setContent] = useState('')
  const [images, setImages] = useState<PostImage[]>([])
  const [isPosting, setIsPosting] = useState(false)
  const { currentProfile } = useProfileStore()
  const addPost = usePostStore((state) => state.addPost)
  const fileInputRef = useRef<HTMLInputElement>(null)

  const handleImageChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
    if (e.target.files) {
      const files = Array.from(e.target.files)
      try {
        const processedImages = await Promise.all(
          files.map(file => processAndStoreImage(file, 'post-image'))
        )
        setImages(prev => [...prev, ...processedImages.filter((img): img is PostImage => img !== null)])
      } catch (error) {
        console.error("Error processing images:", error)
        // Optionally, show an error message to the user
      }
    }
  }

  const removeImage = (id: string) => {
    setImages(prev => prev.filter(img => img.id !== id))
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if ((!content.trim() && images.length === 0) || !currentProfile) return

    setIsPosting(true)
    
    try {
      const authorKey = currentProfile.publicKey || currentProfile.username
      await addPost({
        author: authorKey,
        authorDisplayName: currentProfile.displayName || currentProfile.username,
        authorAvatar: currentProfile.avatar,
        content,
        images,
      })

      console.log('Post created and seeding process initiated.')
      setContent('')
      setImages([])
    } catch (error) {
      console.error('Failed to create post:', error)
    } finally {
      setIsPosting(false)
    }
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

        {images.length > 0 && (
          <div className="grid grid-cols-3 sm:grid-cols-4 md:grid-cols-5 gap-4">
            {images.map(image => (
              <div key={image.id} className="relative group">
                <img src={image.data} alt="Post attachment" className="w-full h-24 object-cover rounded-md" />
                <button
                  type="button"
                  onClick={() => removeImage(image.id)}
                  className="absolute top-1 right-1 bg-black/50 text-white rounded-full p-1 text-xs opacity-0 group-hover:opacity-100 transition-opacity"
                  aria-label="Remove image"
                >
                  ✕
                </button>
              </div>
            ))}
          </div>
        )}
        
        <div className="flex justify-between items-center">
          <div className="flex items-center space-x-2">
            <button
              type="button"
              onClick={() => fileInputRef.current?.click()}
              className="p-2 rounded-full hover:bg-gray-100 dark:hover:bg-gray-700"
              aria-label="Add image"
              disabled={isPosting}
            >
              <svg className="w-6 h-6 text-blue-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" /></svg>
            </button>
            <input
              type="file"
              ref={fileInputRef}
              onChange={handleImageChange}
              className="hidden"
              accept="image/*"
              multiple
              disabled={isPosting}
              aria-label="File uploader"
            />
            <span className="text-sm text-gray-500">Posts are seeded as torrents</span>
          </div>
          <button
            type="submit"
            disabled={isPosting || (!content.trim() && images.length === 0)}
            className="px-6 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-gray-400 text-white font-medium rounded-lg transition-colors"
          >
            {isPosting ? '🌱 Seeding...' : '🌱 Seed Post'}
          </button>
        </div>
      </form>
    </div>
  )
}

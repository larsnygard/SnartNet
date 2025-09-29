// Removed stray contact lookup at top level
import React from 'react'
import { useParams, Link } from 'react-router-dom'
import { useContactStore } from '@/stores/contactStore'
import { usePostStore } from '@/stores/postStore'
import { ImageProcessor } from '@/lib/imageProcessor' // Keep this import if it's used later

const ContactProfilePage: React.FC = () => {
  const { id } = useParams<{ id: string }>()
  const contacts = useContactStore(state => state.contacts)
  const posts = usePostStore(state => state.posts)

  const contact = contacts.find(c => c.id === id)
  const contactPosts = posts.filter(p => p.author === id)

  React.useEffect(() => { (async () => { await (await import('@/stores/contactStore')).loadContacts() })() }, [])
  React.useEffect(() => { (async () => { await (await import('@/stores/postStore')).loadPosts?.() })() }, [])

  if (!contact) {
    return <div className="max-w-2xl mx-auto p-6 text-center text-red-600 dark:text-red-400">Contact not found.</div>;
  }

  return (
    <div className="max-w-4xl mx-auto p-6 space-y-8">
      <div className="flex items-start gap-6 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl p-6 shadow">
        <div className="w-24 h-24 rounded-full bg-gradient-to-br from-blue-500 to-purple-600 flex items-center justify-center text-white text-3xl font-semibold overflow-hidden">
          {contact.avatar ? (
            <img src={contact.avatar} alt={contact.displayName} className="w-24 h-24 object-cover" />
          ) : (
            contact.displayName.charAt(0).toUpperCase()
          )}
        </div>
        <div className="flex-1 min-w-0">
          <h1 className="text-2xl font-bold text-gray-900 dark:text-white mb-1">{contact.displayName}</h1>
          <div className="flex flex-wrap items-center gap-2 text-sm mb-2">
            <span className="text-gray-500 dark:text-gray-400">@{contact.username}</span>
            {contact.postIndexMagnetUri && (
              <span className="text-[10px] px-2 py-0.5 rounded bg-indigo-100 dark:bg-indigo-900/40 text-indigo-700 dark:text-indigo-300">posts</span>
            )}
            <span className="text-[10px] px-2 py-0.5 rounded bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-200">Trust {contact.trustLevel}/10</span>
            <span className="text-[10px] px-2 py-0.5 rounded bg-blue-100 dark:bg-blue-900/40 text-blue-700 dark:text-blue-300">{contact.relationship}</span>
          </div>
          <div className="text-xs text-gray-400 dark:text-gray-500">
            Added {new Date(contact.addedDate).toLocaleDateString()} • {contact.magnetUri.slice(0,42)}…
          </div>
          {contact.notes && contact.notes.trim() && (
            <p className="mt-3 text-sm text-gray-700 dark:text-gray-300 whitespace-pre-line">{contact.notes}</p>
          )}
        </div>
        <div className="flex flex-col gap-2 items-end">
          {contact.postIndexMagnetUri && (
            <span className="text-xs text-green-600 dark:text-green-400">Index ready</span>
          )}
        </div>
      </div>

      <div>
        <h2 className="text-lg font-semibold text-gray-800 dark:text-gray-100 mb-4">Recent Posts</h2>
        {contactPosts.length === 0 ? (
          <div className="text-sm text-gray-500 dark:text-gray-400 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg p-6 text-center">
            No posts downloaded yet. Sync will populate as posts arrive.
          </div>
        ) : (
          <div className="space-y-4">
            {contactPosts.map(post => (
              <div key={post.id} className="bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg p-4">
                <div className="flex items-center justify-between mb-2">
                  <div className="flex items-center gap-2 text-xs text-gray-500 dark:text-gray-400">
                    <span>{new Date(post.createdAt).toLocaleString()}</span>
                    {post.signatureVerified ? (
                      <span className="px-1.5 py-0.5 rounded text-[10px] font-semibold bg-green-100 dark:bg-green-900 text-green-700 dark:text-green-300 border border-green-300 dark:border-green-700">ver</span>
                    ) : (
                      <span className="px-1.5 py-0.5 rounded text-[10px] font-semibold bg-yellow-100 dark:bg-yellow-900 text-yellow-700 dark:text-yellow-300 border border-yellow-300 dark:border-yellow-700" title={post.signatureError || 'unverified'}>unv</span>
                    )}
                    {post.magnetUri && <button onClick={() => navigator.clipboard.writeText(post.magnetUri!)} className="text-blue-600 dark:text-blue-400 hover:underline">🧲</button>}
                  </div>
                </div>
                <p className="text-sm text-gray-900 dark:text-gray-100 whitespace-pre-line mb-2">{post.content}</p>
                {post.images && post.images.length > 0 && (
                  <div className="grid gap-2 grid-cols-2">
                    {post.images.map((img: any) => (
                      <img key={img.id} src={ImageProcessor.base64ToDataUrl(img.data)} className="w-full h-40 object-cover rounded" alt={img.filename} />
                    ))}
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="text-center pt-8">
        <Link to="/network" className="text-sm text-blue-600 dark:text-blue-400 hover:underline">← Back to Network</Link>
      </div>
    </div>
  )
}

export default ContactProfilePage

import React, { useEffect } from 'react'
import { useContactStore } from '@/stores/contactStore'

// Simple friends page focusing on "friend" relationship (and showing ring-of-trust inline)
const FriendsPage: React.FC = () => {
  const { loadContacts, getContactsByRelationship } = useContactStore()
  const friends = getContactsByRelationship('friend')
  const ring = getContactsByRelationship('ring-of-trust')

  useEffect(() => {
    loadContacts()
  }, [loadContacts])

  return (
    <div className="max-w-4xl mx-auto p-6">
      <div className="mb-8">
        <h1 className="text-3xl font-bold text-gray-900 dark:text-white mb-2">Friends</h1>
        <p className="text-gray-600 dark:text-gray-400">People whose posts will appear in your home feed.</p>
      </div>

      <section className="mb-10">
        <header className="flex items-center justify-between mb-4">
          <h2 className="text-xl font-semibold text-gray-900 dark:text-white flex items-center gap-2">👥 Friends <span className="text-sm font-normal text-gray-500 dark:text-gray-400">{friends.length}</span></h2>
        </header>
        {friends.length === 0 ? (
          <div className="p-8 text-center border border-dashed border-gray-300 dark:border-gray-600 rounded-lg text-gray-500 dark:text-gray-400">
            You haven't added any friends yet. Add contacts under the Contacts page and mark them as friends.
          </div>
        ) : (
          <ul className="grid gap-4 sm:grid-cols-2">
            {friends.map(friend => (
              <li key={friend.id} className="bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg p-4 flex items-center gap-4">
                <div className="w-12 h-12 rounded-full bg-gradient-to-br from-blue-500 to-purple-600 flex items-center justify-center text-white text-lg font-semibold">
                  {friend.avatar ? (
                    <img src={friend.avatar} alt={friend.displayName} className="w-12 h-12 rounded-full object-cover" />
                  ) : friend.displayName.charAt(0).toUpperCase()}
                </div>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2 flex-wrap">
                    <span className="font-medium text-gray-900 dark:text-white truncate">{friend.displayName}</span>
                    <span className="text-xs px-2 py-0.5 rounded-full bg-green-100 dark:bg-green-900/40 text-green-700 dark:text-green-300">friend</span>
                  </div>
                  <div className="text-sm text-gray-500 dark:text-gray-400 truncate">@{friend.username}</div>
                  <div className="mt-1 text-xs text-gray-400 dark:text-gray-500">Trust {friend.trustLevel}/10</div>
                </div>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section>
        <header className="flex items-center justify-between mb-4">
          <h2 className="text-xl font-semibold text-gray-900 dark:text-white flex items-center gap-2">🛡️ Ring of Trust <span className="text-sm font-normal text-gray-500 dark:text-gray-400">{ring.length}</span></h2>
        </header>
        {ring.length === 0 ? (
          <div className="p-6 text-sm text-gray-500 dark:text-gray-400 border border-dashed border-gray-300 dark:border-gray-600 rounded-lg">
            No ring-of-trust contacts yet.
          </div>
        ) : (
          <ul className="space-y-3">
            {ring.map(contact => (
              <li key={contact.id} className="bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg p-4 flex items-center gap-4">
                <div className="w-10 h-10 rounded-full bg-red-500/90 flex items-center justify-center text-white text-sm font-semibold">
                  {contact.displayName.charAt(0).toUpperCase()}
                </div>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2 flex-wrap">
                    <span className="font-medium text-gray-900 dark:text-white truncate">{contact.displayName}</span>
                    <span className="text-xs px-2 py-0.5 rounded-full bg-red-100 dark:bg-red-900/40 text-red-700 dark:text-red-300">ring-of-trust</span>
                  </div>
                  <div className="text-sm text-gray-500 dark:text-gray-400 truncate">@{contact.username}</div>
                  <div className="mt-1 text-xs text-gray-400 dark:text-gray-500">Trust {contact.trustLevel}/10 • Key recovery enabled</div>
                </div>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  )
}

export default FriendsPage

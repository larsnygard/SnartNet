import React, { useState } from 'react'
import { useContactStore } from '@/stores/contactStore'
import { useMessageStore } from '@/stores/messageStore'
import ChatThread from '@/components/ChatThread'

const MessagesPage: React.FC = () => {
  const { contacts } = useContactStore()
  const [activeContactId, setActiveContactId] = useState<string | null>(null)
  const threads = useMessageStore(state => state.threads)

  const activeThread = activeContactId ? threads[activeContactId] : null

  return (
    <div className="flex h-[80vh] border rounded-lg overflow-hidden bg-white dark:bg-gray-900">
      {/* Contact list */}
      <div className="w-64 border-r bg-gray-50 dark:bg-gray-800 p-4 overflow-y-auto">
        <h2 className="font-bold mb-4 text-gray-700 dark:text-gray-200">Messages</h2>
        <ul>
          {contacts.map(c => (
            <li key={c.id || c.username}>
              <button
                className={`w-full text-left px-2 py-2 rounded hover:bg-blue-100 dark:hover:bg-blue-900 ${activeContactId === c.id ? 'bg-blue-100 dark:bg-blue-900' : ''}`}
                onClick={() => setActiveContactId(c.id)}
              >
                <span className="font-medium">{c.displayName || c.username}</span>
              </button>
            </li>
          ))}
        </ul>
      </div>
      {/* Chat area */}
      <div className="flex-1 flex flex-col">
        {activeThread ? (
          <ChatThread contactId={activeThread.contactId} />
        ) : (
          <div className="flex-1 flex items-center justify-center text-gray-400">
            Select a contact to start chatting
          </div>
        )}
      </div>
    </div>
  )
}

export default MessagesPage
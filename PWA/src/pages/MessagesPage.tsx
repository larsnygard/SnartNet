import React, { useState, useEffect } from 'react'
import { useContactStore } from '@/stores/contactStore'
import ChatThread from '@/components/ChatThread'

const MessagesPage: React.FC = () => {
  const { contacts } = useContactStore()
  const [activeContactId, setActiveContactId] = useState<string | null>(null)
  // Auto select first contact if none selected yet
  useEffect(() => {
    if (!activeContactId && contacts.length > 0) {
      setActiveContactId(contacts[0].id)
    }
  }, [activeContactId, contacts])

  return (
    <div className="flex h-[80vh] border rounded-lg overflow-hidden bg-white dark:bg-gray-900">
      {/* Contact list (hidden on small screens when a contact is active) */}
      <div className={`md:w-64 w-full md:block ${activeContactId ? 'hidden md:block' : 'block'} border-r bg-gray-50 dark:bg-gray-800 p-4 overflow-y-auto`}>        
        <h2 className="font-bold mb-4 text-gray-700 dark:text-gray-200">Messages</h2>
        {contacts.length === 0 && (
          <p className="text-sm text-gray-500 dark:text-gray-400">Add contacts to start messaging.</p>
        )}
        <ul className="space-y-1">
          {contacts.map(c => (
            <li key={c.id || c.username}>
              <button
                className={`w-full text-left px-3 py-2 rounded transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 text-sm font-medium shadow-sm border border-transparent ${activeContactId === c.id 
                  ? 'bg-blue-600 dark:bg-blue-500 text-white' 
                  : 'bg-white dark:bg-gray-700 text-gray-700 dark:text-gray-100 hover:bg-blue-50 dark:hover:bg-gray-600'} `}
                onClick={() => setActiveContactId(c.id)}
                aria-current={activeContactId === c.id ? 'true' : 'false'}
              >
                {c.displayName || c.username}
              </button>
            </li>
          ))}
        </ul>
      </div>
      {/* Chat area */}
      <div className="flex-1 flex flex-col">
        {activeContactId ? (
          <ChatThread contactId={activeContactId} onBackMobile={() => setActiveContactId(null)} />
        ) : (
          <div className="flex-1 hidden md:flex items-center justify-center text-gray-400">
            Select a contact to start chatting
          </div>
        )}
      </div>
    </div>
  )
}

export default MessagesPage
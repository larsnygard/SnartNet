import React, { useRef, useState } from 'react'
import { useContactStore } from '@/stores/contactStore'
import { useMessageStore } from '@/stores/messageStore'

const ChatThread: React.FC<{ contactId: string }> = ({ contactId }) => {
  const { contacts } = useContactStore()
  const thread = useMessageStore(state => state.threads[contactId])
  const sendMessage = useMessageStore(state => state.sendMessage)
  const [input, setInput] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)
  const contact = contacts.find(c => c.id === contactId)

  const handleSend = async (e: React.FormEvent) => {
    e.preventDefault()
    if (input.trim()) {
      await sendMessage(contactId, input.trim())
      setInput('')
      inputRef.current?.focus()
    }
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center border-b px-4 py-2 bg-gray-100 dark:bg-gray-800">
        <span className="font-semibold text-lg text-gray-800 dark:text-gray-100">{contact?.displayName || contact?.username}</span>
      </div>
      <div className="flex-1 overflow-y-auto p-4 space-y-2 bg-white dark:bg-gray-900">
        {thread?.messages.map(msg => (
          <div key={msg.id} className={`flex ${msg.sender === 'me' ? 'justify-end' : 'justify-start'}`}>
            <div className={`max-w-xs px-3 py-2 rounded-lg shadow text-sm ${msg.sender === 'me' ? 'bg-blue-500 text-white' : 'bg-gray-200 dark:bg-gray-700 text-gray-900 dark:text-gray-100'}`}>
              <div>{msg.content}</div>
              <div className="text-[10px] mt-1 text-right opacity-60">
                {new Date(msg.timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                {msg.status === 'pending' && <span className="ml-1">⏳</span>}
                {msg.status === 'failed' && <span className="ml-1 text-red-500">⚠️</span>}
              </div>
            </div>
          </div>
        ))}
      </div>
      <form onSubmit={handleSend} className="flex gap-2 p-4 border-t bg-gray-50 dark:bg-gray-800">
        <input
          ref={inputRef}
          type="text"
          className="flex-1 px-3 py-2 rounded border bg-white dark:bg-gray-900 text-gray-900 dark:text-gray-100"
          placeholder="Type a message..."
          value={input}
          onChange={e => setInput(e.target.value)}
        />
        <button type="submit" className="px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-700">Send</button>
      </form>
    </div>
  )
}

export default ChatThread

import { create } from 'zustand'
import { nanoid } from 'nanoid'
import { publishEncryptedMessage, onMessage } from '@/lib/push/messages'
import { writeMessageThread } from '@/lib/persistence'

export interface Message {
  id: string
  sender: string // public key or username
  recipient: string // public key or username
  content: string
  timestamp: string // ISO
  encrypted?: boolean
  signature?: string
  status?: 'pending' | 'sent' | 'delivered' | 'read' | 'failed'
}

interface MessageThread {
  contactId: string
  messages: Message[]
}

interface MessageState {
  threads: Record<string, MessageThread>
  sendMessage: (contactId: string, content: string) => Promise<void>
  receiveMessage: (contactId: string, message: Message) => void
  loadMessages: (contactId: string) => void
  setMessages: (contactId: string, messages: Message[]) => void
}

export const useMessageStore = create<MessageState>((set) => ({
  threads: {},

  async sendMessage(contactId, content) {
    try {
      const localMsg = await publishEncryptedMessage(contactId, content)
      // Stored via onMessage callback as well, but we insert optimistic now
      set(state => ({
        threads: {
          ...state.threads,
          [contactId]: {
            contactId,
            messages: [...(state.threads[contactId]?.messages || []), localMsg]
          }
        }
      }))
      writeMessageThread(contactId, useMessageStore.getState().threads[contactId].messages).catch(()=>{})
    } catch (e) {
      const failed: Message = {
        id: nanoid(),
        sender: 'me',
        recipient: contactId,
        content,
        timestamp: new Date().toISOString(),
        status: 'failed'
      }
      set(state => ({
        threads: {
          ...state.threads,
          [contactId]: {
            contactId,
            messages: [...(state.threads[contactId]?.messages || []), failed]
          }
        }
      }))
    }
  },


  receiveMessage(contactId, message) {
    set(state => {
      const thread = state.threads[contactId]
      const updated = {
        threads: {
          ...state.threads,
          [contactId]: {
            contactId,
            messages: [...(thread?.messages || []), message]
          }
        }
      }
      // Persist after state mutation (async fire & forget)
      setTimeout(() => {
        try { writeMessageThread(contactId, updated.threads[contactId].messages).catch(()=>{}) } catch {}
      }, 0)
      return updated
    })
  },

  loadMessages() {
    // TODO: Load from localStorage/IndexedDB
  },

  setMessages(contactId, messages) {
    set(state => ({
      threads: {
        ...state.threads,
        [contactId]: {
          contactId,
          messages
        }
      }
    }))
  }
}))

// Listen for incoming messages via push transport (must be outside the store definition)
onMessage((msg) => {
  // Determine thread key: if current user is sender, thread is recipient; else sender.
  const threadId = msg.sender === 'me' ? msg.recipient : msg.sender
  useMessageStore.getState().receiveMessage(threadId, msg)
})

import { create } from 'zustand'
import { nanoid } from 'nanoid'
import { publishMessage, onMessage } from '@/lib/push/messages'

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
    // TODO: Encrypt, sign, and send via push or torrent
    const msg: Message = {
      id: nanoid(),
      sender: 'me', // TODO: use real sender id
      recipient: contactId,
      content,
      timestamp: new Date().toISOString(),
      status: 'pending',
    }
    set(state => ({
      threads: {
        ...state.threads,
        [contactId]: {
          contactId,
          messages: [...(state.threads[contactId]?.messages || []), msg]
        }
      }
    }))
    try {
      await publishMessage(msg)
      set(state => ({
        threads: {
          ...state.threads,
          [contactId]: {
            contactId,
            messages: state.threads[contactId].messages.map(m => m.id === msg.id ? { ...m, status: 'sent' } : m)
          }
        }
      }))
    } catch (e) {
      set(state => ({
        threads: {
          ...state.threads,
          [contactId]: {
            contactId,
            messages: state.threads[contactId].messages.map(m => m.id === msg.id ? { ...m, status: 'failed' } : m)
          }
        }
      }))
    }
  },


  receiveMessage(contactId, message) {
    set(state => ({
      threads: {
        ...state.threads,
        [contactId]: {
          contactId,
          messages: [...(state.threads[contactId]?.messages || []), message]
        }
      }
    }))
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
  useMessageStore.getState().receiveMessage(msg.sender, msg)
})

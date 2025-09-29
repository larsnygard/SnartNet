import { create } from 'zustand';
import { nanoid } from 'nanoid';
import { publishMessage, onMessage } from '@/lib/push/messages';
import { SnartStorage } from '../lib/SnartStorage';

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
  threads: Record<string, MessageThread>;
  setMessages: (contactId: string, messages: Message[]) => void;
  receiveMessage: (contactId: string, message: Message) => void;
  sendMessage: (contactId: string, content: string) => Promise<void>;
}

// Async helpers for file IO
export const MESSAGES_DIR = '/messages';
const storage = new SnartStorage();

export async function loadMessagesFromFS(contactId: string): Promise<Message[]> {
  try {
    const file = `${MESSAGES_DIR}/${contactId}.json`;
    if (await storage.fileExists(file)) {
      const data = await storage.readFile(file);
      return JSON.parse(data);
    }
  } catch (e) { console.warn('Failed to load messages for', contactId, e); }
  return [];
}

export async function saveMessagesToFS(contactId: string, messages: Message[]) {
  await storage.writeFile(`${MESSAGES_DIR}/${contactId}.json`, JSON.stringify(messages));
}

export const useMessageStore = create<MessageState>((set) => ({
  threads: {},
  setMessages: (contactId, messages) => set(state => ({
    threads: {
      ...state.threads,
      [contactId]: { contactId, messages }
    }
  })),
  receiveMessage: (contactId, message) => set(state => ({
    threads: {
      ...state.threads,
      [contactId]: {
        contactId,
        messages: [...(state.threads[contactId]?.messages || []), message]
      }
    }
  })),
  sendMessage: async (contactId, content) => {
    await sendMessage(contactId, content);
  },
}));

// Async helpers for actions
export async function sendMessage(contactId: string, content: string) {
  const msg: Message = {
    id: nanoid(),
    sender: 'me', // TODO: use real sender id
    recipient: contactId,
    content,
    timestamp: new Date().toISOString(),
    status: 'pending',
  };
  const messages = await loadMessagesFromFS(contactId);
  messages.push(msg);
  await saveMessagesToFS(contactId, messages);
  useMessageStore.getState().setMessages(contactId, messages);
  try {
    await publishMessage(msg);
    msg.status = 'sent';
    await saveMessagesToFS(contactId, messages);
    useMessageStore.getState().setMessages(contactId, messages);
  } catch (e) {
    msg.status = 'failed';
    await saveMessagesToFS(contactId, messages);
    useMessageStore.getState().setMessages(contactId, messages);
  }
}

export async function loadMessages(contactId: string) {
  const messages = await loadMessagesFromFS(contactId);
  useMessageStore.getState().setMessages(contactId, messages);
}

// Listen for incoming messages via push transport (must be outside the store definition)
onMessage((msg) => {
  useMessageStore.getState().receiveMessage(msg.sender, msg);
  // Optionally persist received messages
  loadMessages(msg.sender);
});

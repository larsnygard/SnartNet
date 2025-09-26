import { create } from 'zustand'

export type RelationshipType = 'ring-of-trust' | 'friend' | 'acquaintance' | 'group-member'

export interface ContactPermissions {
  canMessage: boolean
  canSeeFullProfile: boolean
  canShareContacts: boolean
  canRecoverKeys: boolean // Ring of Trust only
}

export interface Contact {
  id: string // Public key fingerprint
  username: string
  displayName: string
  relationship: RelationshipType
  trustLevel: number // 1-10 scale
  addedDate: string
  lastSeen?: string
  magnetUri: string
  avatar?: string
  notes?: string
  permissions: ContactPermissions
  // Optional magnet URI to the head of this contact's post index chain
  postIndexMagnetUri?: string
  // Sync preferences
  syncMaxPosts?: number // e.g. 100
  syncMonthsLookback?: number // e.g. 5 (last 5 months)
}

interface ContactState {
  contacts: Contact[]
  loadContacts: () => void
  addContact: (contact: Omit<Contact, 'id' | 'addedDate' | 'permissions'>) => void
  removeContact: (contactId: string) => void
  updateContact: (contactId: string, updates: Partial<Contact>) => void
  getContactsByRelationship: (relationship: RelationshipType) => Contact[]
  getContact: (contactId: string) => Contact | undefined
}

// Default permissions based on relationship type
const getDefaultPermissions = (relationship: RelationshipType): ContactPermissions => {
  switch (relationship) {
    case 'ring-of-trust':
      return {
        canMessage: true,
        canSeeFullProfile: true,
        canShareContacts: true,
        canRecoverKeys: true
      }
    case 'friend':
      return {
        canMessage: true,
        canSeeFullProfile: true,
        canShareContacts: true,
        canRecoverKeys: false
      }
    case 'acquaintance':
      return {
        canMessage: true,
        canSeeFullProfile: false,
        canShareContacts: false,
        canRecoverKeys: false
      }
    case 'group-member':
      return {
        canMessage: true,
        canSeeFullProfile: false,
        canShareContacts: false,
        canRecoverKeys: false
      }
  }
}

// Generate contact ID from username and public key fingerprint
const generateContactId = (username: string, magnetUri: string): string => {
  // Extract some identifying info from magnet URI or use username
  const hash = btoa(username + magnetUri).slice(0, 16)
  return `contact_${hash}`
}

export const useContactStore = create<ContactState>((set, get) => ({
  contacts: [],

  loadContacts: () => {
    try {
      const stored = localStorage.getItem('snartnet-contacts')
      if (stored) {
        const contacts = JSON.parse(stored)
        set({ contacts })
      }
    } catch (error) {
      console.error('Failed to load contacts:', error)
    }
  },

  addContact: (contactData) => {
    const contacts = get().contacts
    
    // Generate ID and add timestamps
    const newContact: Contact = {
      ...contactData,
      id: generateContactId(contactData.username, contactData.magnetUri),
      addedDate: new Date().toISOString(),
      permissions: getDefaultPermissions(contactData.relationship)
    }

    // Check if contact already exists
    const existingIndex = contacts.findIndex(c => c.id === newContact.id)
    if (existingIndex >= 0) {
      // Update existing contact
      const updatedContacts = [...contacts]
      updatedContacts[existingIndex] = { ...updatedContacts[existingIndex], ...newContact }
      set({ contacts: updatedContacts })
      localStorage.setItem('snartnet-contacts', JSON.stringify(updatedContacts))
    } else {
      // Add new contact
      const updatedContacts = [...contacts, newContact]
      set({ contacts: updatedContacts })
      localStorage.setItem('snartnet-contacts', JSON.stringify(updatedContacts))
    }
  },

  removeContact: (contactId) => {
    const contacts = get().contacts
    const updatedContacts = contacts.filter(c => c.id !== contactId)
    set({ contacts: updatedContacts })
    localStorage.setItem('snartnet-contacts', JSON.stringify(updatedContacts))
  },

  updateContact: (contactId, updates) => {
    const contacts = get().contacts
    const updatedContacts = contacts.map(contact => 
      contact.id === contactId 
        ? { ...contact, ...updates }
        : contact
    )
    set({ contacts: updatedContacts })
    localStorage.setItem('snartnet-contacts', JSON.stringify(updatedContacts))
  },

  getContactsByRelationship: (relationship) => {
    return get().contacts.filter(c => c.relationship === relationship)
  },

  getContact: (contactId) => {
    return get().contacts.find(c => c.id === contactId)
  }
}))
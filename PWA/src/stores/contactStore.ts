// Handle incoming head update event
export async function handleIncomingHeadUpdate(evt: any) {
  // Import verify util lazily
  const { verifyHeadUpdateSignature } = await import('@/lib/crypto/headUpdate');
  const { usePostStore } = await import('./postStore');
  // Basic shape check
  if (!evt || evt.kind !== 'postIndexHeadUpdate' || !evt.profileId || !evt.newHead || !evt.signature) {
    return;
  }
  // Replay / dedupe cache (in-memory). Keep last 200 signatures.
  const g: any = (window as any);
  if (!g.__sn_head_sig_cache) g.__sn_head_sig_cache = [];
  if (g.__sn_head_sig_cache.includes(evt.signature)) return;
  const { ok } = await verifyHeadUpdateSignature(evt);
  if (!ok) {
    console.warn('Rejected head update: invalid signature', evt);
    return;
  }
  // Insert into cache
  g.__sn_head_sig_cache.push(evt.signature);
  if (g.__sn_head_sig_cache.length > 200) g.__sn_head_sig_cache.splice(0, g.__sn_head_sig_cache.length - 200);
  // Flood control: track per profileId events per minute
  if (!g.__sn_head_rate) g.__sn_head_rate = {};
  const now = Date.now();
  const windowMs = 60_000;
  const bucketKey = evt.profileId;
  const rate = g.__sn_head_rate[bucketKey] || { start: now, count: 0 };
  if (now - rate.start > windowMs) {
    rate.start = now; rate.count = 0;
  }
  rate.count++;
  g.__sn_head_rate[bucketKey] = rate;
  if (rate.count > 30) { // arbitrary limit: 30 head updates / minute per profile
    if (!g.__sn_head_rate_warned) g.__sn_head_rate_warned = new Set();
    if (!g.__sn_head_rate_warned.has(bucketKey)) {
      console.warn('Rate limiting head updates for', bucketKey);
      g.__sn_head_rate_warned.add(bucketKey);
    }
    return;
  }
  // Timestamp freshness check (allow 5 min skew)
  try {
    const issued = new Date(evt.issuedAt).getTime();
    if (!isNaN(issued)) {
      const skew = Math.abs(Date.now() - issued);
      if (skew > 5 * 60_000) {
        console.warn('Discarding stale/future head update (skew >5m)', evt);
        return;
      }
  }
  } catch {}
  // Find contact by profileId (username)
  const contact = useContactStore.getState().contacts.find(c => c.username === evt.profileId);
  if (!contact) return;
  // Only update if new head is different and issuedAt is newer
  if (contact.postIndexMagnetUri !== evt.newHead) {
    // Optionally: check issuedAt > last seen (not tracked yet)
    useContactStore.getState().updateContact(contact.id, { postIndexMagnetUri: evt.newHead });
    // Trigger targeted sync
    usePostStore.getState().syncPostsForContact(contact.id, { maxPosts: contact.syncMaxPosts });
  }
}
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
  addContactFromMagnet: (magnetUri: string, relationship?: RelationshipType) => Promise<Contact | null>
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
        let contacts = JSON.parse(stored)
        // Ensure all contacts have a valid id
        contacts = contacts.map((c: any) => ({
          ...c,
          id: c.id || generateContactId(c.username, c.magnetUri)
        }))
        set({ contacts })
        localStorage.setItem('snartnet-contacts', JSON.stringify(contacts))
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

  addContactFromMagnet: async (magnetUri, relationship = 'friend') => {
    try {
      // Optimistically create placeholder while we fetch
      const tempId = `pending_${Math.random().toString(36).slice(2)}`
      const placeholder: Contact = {
        id: tempId,
        username: 'loading…',
        displayName: 'Loading profile…',
        relationship,
        trustLevel: relationship === 'ring-of-trust' ? 8 : 5,
        addedDate: new Date().toISOString(),
        magnetUri,
        permissions: getDefaultPermissions(relationship),
      }
      const existing = get().contacts
      set({ contacts: [...existing, placeholder] })

      // Dynamically import torrent service accessor to avoid circular import concerns
      const { getTorrentService } = await import('@/lib/torrent')
      const svc: any = getTorrentService()
      const profile = await svc.downloadProfile(magnetUri)
      if (!profile) {
        // Mark placeholder as failed
        set({ contacts: get().contacts.map(c => c.id === tempId ? { ...c, username: 'unknown', displayName: 'Profile unavailable' } : c) })
        return null
      }

      const username = profile.username || 'unknown'
      const displayName = profile.displayName || username
      const avatar = profile.avatar || profile.profilePicture || undefined
      const finalContact: Omit<Contact, 'id' | 'addedDate' | 'permissions'> = {
        username,
        displayName,
        relationship,
        trustLevel: relationship === 'ring-of-trust' ? 8 : 5,
        magnetUri,
        avatar,
        notes: '',
        postIndexMagnetUri: (profile as any).postIndexMagnetUri || (profile as any).post_index_magnet || undefined,
        syncMaxPosts: 100,
        syncMonthsLookback: 6,
      }
      // Remove placeholder then add real contact
      set({ contacts: get().contacts.filter(c => c.id !== tempId) })
      get().addContact(finalContact)
      const addedContact = get().contacts.find(c => c.username === username && c.magnetUri === magnetUri)
      
      // Auto-sync posts for the new contact if they have a postIndexMagnetUri
      if (addedContact && finalContact.postIndexMagnetUri) {
        // Dynamically import post store to avoid circular dependencies
        const { usePostStore } = await import('./postStore')
        try {
          await usePostStore.getState().syncPostsForContact(addedContact.id, {
            maxPosts: finalContact.syncMaxPosts || 50,
            monthsLookback: finalContact.syncMonthsLookback || 6
          })
          console.log('[ContactStore] Auto-synced posts for new contact:', username)
        } catch (error) {
          console.warn('[ContactStore] Failed to auto-sync posts for new contact:', error)
        }
      }
      
      return addedContact || null
    } catch (e) {
      console.error('addContactFromMagnet failed', e)
      // Remove placeholder if present
      set({ contacts: get().contacts.filter(c => !c.id.startsWith('pending_')) })
      return null
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
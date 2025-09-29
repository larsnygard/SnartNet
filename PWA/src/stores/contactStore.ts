import { create } from 'zustand';
import { getTorrentService } from '@/lib/torrent';
import { SnartStorage } from '../lib/SnartStorage';

export type RelationshipType = 'ring-of-trust' | 'friend' | 'acquaintance' | 'group-member';

export interface ContactPermissions {
  canMessage: boolean;
  canSeeFullProfile: boolean;
  canShareContacts: boolean;
  canRecoverKeys: boolean;
}

export interface SeededTorrentMeta {
  magnetUri: string;
  infoHash?: string;
  name?: string;
  size?: number;
  addedAt: string;
}

export interface Contact {
  id: string;
  username: string;
  displayName: string;
  relationship: RelationshipType;
  trustLevel: number;
  addedDate: string;
  lastSeen?: string;
  magnetUri: string;
  avatar?: string;
  notes?: string;
  permissions: ContactPermissions;
  postIndexMagnetUri?: string;
  syncMaxPosts?: number;
  syncMonthsLookback?: number;
  storageLimitMB?: number | null;
  seededTorrents?: SeededTorrentMeta[];
  storageUsed?: number;
}

export interface ContactState {
  contacts: Contact[];
  setContacts: (contacts: Contact[]) => void;
  getContactsByRelationship: (relationship: RelationshipType) => Contact[];
  getContact: (contactId: string) => Contact | undefined;
}

// Async helpers for file IO
export const CONTACTS_DIR = '/contacts';
const storage = new SnartStorage();

export async function loadContactsFromFS(): Promise<Contact[]> {
  let files: string[] = [];
  try {
    files = await storage.listFiles(CONTACTS_DIR);
  } catch (e) {
    // Directory may not exist yet
    return [];
  }
  const contacts: Contact[] = [];
  for (const file of files) {
    try {
      const data = await storage.readFile(file);
      if (data) contacts.push(JSON.parse(data));
    } catch (e) { console.warn('Failed to load contact', file, e); }
  }
  return contacts;
}

export async function saveContactToFS(contact: Contact) {
  await storage.writeFile(`${CONTACTS_DIR}/${contact.id}.json`, JSON.stringify(contact));
}

export async function removeContactFromFS(contactId: string) {
  await storage.deleteFile(`${CONTACTS_DIR}/${contactId}.json`);
}

// Generate contact ID from username and public key fingerprint
const generateContactId = (username: string, magnetUri: string): string => {
  const hash = btoa(username + magnetUri).slice(0, 16);
  return `contact_${hash}`;
};

export const useContactStore = create<ContactState>((set, get) => ({
  contacts: [],
  setContacts: (contacts) => set({ contacts }),
  getContactsByRelationship: (relationship) => get().contacts.filter(c => c.relationship === relationship),
  getContact: (contactId) => get().contacts.find(c => c.id === contactId),
}));

// Async helpers for actions
export async function addContact(contactData: Omit<Contact, 'id' | 'addedDate' | 'permissions'>) {
  const contacts = await loadContactsFromFS();
  const newContact: Contact = {
    ...contactData,
    id: generateContactId(contactData.username, contactData.magnetUri),
    addedDate: new Date().toISOString(),
    permissions: getDefaultPermissions(contactData.relationship),
    storageLimitMB: contactData.storageLimitMB ?? null,
    seededTorrents: contactData.seededTorrents ?? [],
    storageUsed: contactData.storageUsed ?? 0,
  };
  const existingIndex = contacts.findIndex((c: Contact) => c.id === newContact.id);
  if (existingIndex >= 0) {
    contacts[existingIndex] = { ...contacts[existingIndex], ...newContact };
  } else {
    contacts.push(newContact);
  }
  await saveContactToFS(newContact);
  useContactStore.getState().setContacts(contacts);
}

export async function updateContact(contactId: string, updates: Partial<Contact>) {
  const contacts = await loadContactsFromFS();
  const idx = contacts.findIndex(c => c.id === contactId);
  if (idx >= 0) {
    const updated = { ...contacts[idx], ...updates };
    await saveContactToFS(updated);
    contacts[idx] = updated;
    useContactStore.getState().setContacts(contacts);
  }
}

export async function removeContact(contactId: string) {
  await removeContactFromFS(contactId);
  const contacts = (await loadContactsFromFS()).filter(c => c.id !== contactId);
  useContactStore.getState().setContacts(contacts);
}

export async function loadContacts() {
  const contacts = await loadContactsFromFS();
  useContactStore.getState().setContacts(contacts);
}
// (Removed legacy addContactFromMagnet, removeContact, updateContact, getContactsByRelationship, getContact, and all localStorage usage)

export function getDefaultPermissions(relationship: RelationshipType): ContactPermissions {
  switch (relationship) {
    case 'ring-of-trust':
      return { canMessage: true, canSeeFullProfile: true, canShareContacts: true, canRecoverKeys: true };
    case 'friend':
      return { canMessage: true, canSeeFullProfile: true, canShareContacts: true, canRecoverKeys: false };
    case 'acquaintance':
      return { canMessage: true, canSeeFullProfile: false, canShareContacts: false, canRecoverKeys: false };
    case 'group-member':
      return { canMessage: true, canSeeFullProfile: false, canShareContacts: false, canRecoverKeys: false };
    default:
      return { canMessage: false, canSeeFullProfile: false, canShareContacts: false, canRecoverKeys: false };
  }
}

export async function reseedAllContactTorrents(): Promise<void> {
  const store = useContactStore.getState();
  const contacts: Contact[] = store.contacts || [];
  const torrentService = getTorrentService();
  await new Promise((res) => setTimeout(res, 500));
  contacts.forEach((contact: Contact) => {
    if (contact.seededTorrents && Array.isArray(contact.seededTorrents)) {
      contact.seededTorrents.forEach((torrentMeta: SeededTorrentMeta) => {
        const isActive = torrentService.getActiveTorrents().some((t: any) => t.infoHash === torrentMeta.infoHash);
        if (!isActive && torrentMeta.magnetUri) {
          try {
            torrentService.addMagnet(torrentMeta.magnetUri);
          } catch (e) {
            // Ignore errors for now
          }
        }
      });
    }
  });
}

export function addSeededTorrentToContact(contactId: string, torrent: SeededTorrentMeta): void {
  const store = useContactStore.getState();
  const contact = store.getContact(contactId);
  if (!contact) return;
  const seededTorrents: SeededTorrentMeta[] = [...(contact.seededTorrents || []), torrent];
  const storageUsed = seededTorrents.reduce((sum: number, t: SeededTorrentMeta) => sum + (t.size || 0), 0);
  let updatedTorrents = seededTorrents;
  let updatedStorage = storageUsed;
  if (contact.storageLimitMB && contact.storageLimitMB > 0) {
    const limitBytes = contact.storageLimitMB * 1024 * 1024;
    while (updatedTorrents.length > 0 && updatedStorage > limitBytes) {
      const removed = updatedTorrents.shift();
      updatedStorage -= removed?.size || 0;
    }
  }
    void updateContact(contactId, {
      seededTorrents: updatedTorrents,
      storageUsed: updatedStorage,
    });
}

export function removeSeededTorrentFromContact(contactId: string, infoHash: string): void {
  const store = useContactStore.getState();
  const contact = store.getContact(contactId);
  if (!contact || !contact.seededTorrents) return;
  const updatedTorrents: SeededTorrentMeta[] = contact.seededTorrents.filter((t: SeededTorrentMeta) => t.infoHash !== infoHash);
  const updatedStorage = updatedTorrents.reduce((sum: number, t: SeededTorrentMeta) => sum + (t.size || 0), 0);
    void updateContact(contactId, {
      seededTorrents: updatedTorrents,
      storageUsed: updatedStorage,
    });
}

export async function handleIncomingHeadUpdate(evt: any) {
  const { verifyHeadUpdateSignature } = await import('@/lib/crypto/headUpdate');
  if (!evt || evt.kind !== 'postIndexHeadUpdate' || !evt.profileId || !evt.newHead || !evt.signature) {
    return;
  }
  const g: any = (window as any);
  if (!g.__sn_head_sig_cache) g.__sn_head_sig_cache = [];
  if (g.__sn_head_sig_cache.includes(evt.signature)) return;
  const { ok } = await verifyHeadUpdateSignature(evt);
  if (!ok) {
    console.warn('Rejected head update: invalid signature', evt);
    return;
  }
  g.__sn_head_sig_cache.push(evt.signature);
  if (g.__sn_head_sig_cache.length > 200) g.__sn_head_sig_cache.splice(0, g.__sn_head_sig_cache.length - 200);
  if (!g.__sn_head_rate) g.__sn_head_rate = {};
  const now = Date.now();
  const windowMs = 60_000;
  const bucketKey = evt.profileId;
  const rate = g.__sn_head_rate[bucketKey] || { start: now, count: 0 };
  if (now - rate.start > windowMs) {
    rate.start = now; rate.count = 0;
  }
  g.__sn_head_rate[bucketKey] = rate;
  if (rate.count > 30) {
    if (!g.__sn_head_rate_warned) g.__sn_head_rate_warned = new Set();
    if (!g.__sn_head_rate_warned.has(bucketKey)) {
      console.warn('Rate limiting head updates for', bucketKey);
      g.__sn_head_rate_warned.add(bucketKey);
    }
    return;
  }
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
  const contact = useContactStore.getState().contacts.find((c: Contact) => c.username === evt.profileId);
  if (!contact) return;
  if (contact.postIndexMagnetUri !== evt.newHead) {
    void updateContact(contact.id, { postIndexMagnetUri: evt.newHead });
    // TODO: trigger post sync for contact.id if needed
  }
  // End of file
}
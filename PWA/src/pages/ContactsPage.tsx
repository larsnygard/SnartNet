import React, { useState, useEffect } from 'react'
import MagnetLinkManager from '@/components/MagnetLinkManager'
import QRCodeManager from '@/components/QRCodeManager'
import { useContactStore, type Contact, type RelationshipType } from '../stores/contactStore'


const ContactsPage: React.FC = () => {

  const getContactsByRelationship = useContactStore(state => state.getContactsByRelationship)
  // Use async helpers for contact actions
  // import { loadContacts, addContact, removeContact, updateContact } from '../stores/contactStore'
  // import { loadPosts } from '../stores/postStore'
  
  const [activeTab, setActiveTab] = useState<RelationshipType>('friend')
  const [showAddForm, setShowAddForm] = useState(false)
  
  // Handle contact addition with automatic post sync
  const handleContactAdded = async () => {
    const { loadContacts } = await import('../stores/contactStore')
    const { loadPosts } = await import('../stores/postStore')
    await loadContacts()
    setTimeout(() => loadPosts?.(), 100)
  }

  useEffect(() => {
    (async () => {
      const { loadContacts } = await import('../stores/contactStore')
      await loadContacts()
    })()
  }, [])

  const tabs = [
    { id: 'friend' as RelationshipType, label: 'Friends', icon: '👥' },
    { id: 'acquaintance' as RelationshipType, label: 'Acquaintances', icon: '🤝' },
    { id: 'ring-of-trust' as RelationshipType, label: 'Ring of Trust', icon: '🛡️' },
    { id: 'group-member' as RelationshipType, label: 'Groups', icon: '🏠' }
  ]

  const currentContacts = getContactsByRelationship(activeTab)

  return (
    <div className="max-w-6xl mx-auto p-6">
      {/* Connect with Friends */}
      <div className="mb-10 space-y-6">
        <div>
          <h2 className="text-lg font-semibold text-gray-900 dark:text-white mb-4">Connect with Friends</h2>
          <QRCodeManager onContactAdded={handleContactAdded} />
        </div>
        
        <div>
          <h3 className="text-md font-medium text-gray-700 dark:text-gray-300 mb-3">Or add via Magnet Link</h3>
          <MagnetLinkManager />
        </div>
      </div>
      <div className="flex justify-between items-center mb-6">
        <h1 className="text-2xl font-bold text-gray-900 dark:text-white">
          Contacts & Relationships
        </h1>
        <button
          onClick={() => setShowAddForm(true)}
          className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg transition-colors"
        >
          Add Contact
        </button>
      </div>

      {/* Tab Navigation */}
      <div className="border-b border-gray-200 dark:border-gray-700 mb-6">
        <nav className="flex space-x-8">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={`py-2 px-1 border-b-2 font-medium text-sm transition-colors ${
                activeTab === tab.id
                  ? 'border-blue-500 text-blue-600 dark:text-blue-400'
                  : 'border-transparent text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-300'
              }`}
            >
              <span className="mr-2">{tab.icon}</span>
              {tab.label}
              <span className="ml-2 bg-gray-100 dark:bg-gray-700 text-gray-900 dark:text-gray-100 rounded-full px-2 py-1 text-xs">
                {getContactsByRelationship(tab.id).length}
              </span>
            </button>
          ))}
        </nav>
      </div>

      {/* Add Contact Form */}
      {showAddForm && (
        <AddContactForm 
          onAdd={async (contactData) => {
            const { addContact } = await import('../stores/contactStore');
            await addContact(contactData);
            handleContactAdded();
          }}
          onCancel={() => setShowAddForm(false)}
          defaultRelationship={activeTab}
        />
      )}

      {/* Contacts List */}
      <div className="space-y-4">
        {currentContacts.length === 0 ? (
          <EmptyState relationship={activeTab} onAddContact={() => setShowAddForm(true)} />
        ) : (
          currentContacts.map((contact) => (
            <ContactCard 
              key={contact.id}
              contact={contact}
              onUpdate={async (contactId, updates) => {
                const { updateContact } = await import('../stores/contactStore');
                await updateContact(contactId, updates);
                handleContactAdded();
              }}
              onRemove={async (contactId) => {
                const { removeContact } = await import('../stores/contactStore');
                await removeContact(contactId);
                handleContactAdded();
              }}
            />
          ))
        )}
      </div>
    </div>
  )
}

interface AddContactFormProps {
  onAdd: (contact: Omit<Contact, 'id' | 'addedDate' | 'permissions'>) => void
  onCancel: () => void
  defaultRelationship: RelationshipType
}

const AddContactForm: React.FC<AddContactFormProps> = ({ onAdd, onCancel, defaultRelationship }) => {
  const [formData, setFormData] = useState({
    username: '',
    displayName: '',
    relationship: defaultRelationship,
    trustLevel: 5,
    magnetUri: '',
    notes: ''
  })

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (!formData.username || !formData.magnetUri) return

    onAdd({
      username: formData.username,
      displayName: formData.displayName || formData.username,
      relationship: formData.relationship,
      trustLevel: formData.trustLevel,
      magnetUri: formData.magnetUri,
      notes: formData.notes
    } as Omit<Contact, 'id' | 'addedDate' | 'permissions'>)

    onCancel()
  }

  return (
    <div className="bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg p-6 mb-6">
      <h3 className="text-lg font-semibold text-gray-900 dark:text-white mb-4">Add New Contact</h3>
      
      <form onSubmit={handleSubmit} className="space-y-4">
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
              Username *
            </label>
            <input
              type="text"
              value={formData.username}
              onChange={(e) => setFormData(prev => ({ ...prev, username: e.target.value }))}
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
              required
              placeholder="Enter username"
              title="Username"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
              Display Name
            </label>
            <input
              type="text"
              value={formData.displayName}
              onChange={(e) => setFormData(prev => ({ ...prev, displayName: e.target.value }))}
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
              placeholder="Enter display name (optional)"
              title="Display Name"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
              Relationship Type
            </label>
            <select
              value={formData.relationship}
              onChange={(e) => setFormData(prev => ({ ...prev, relationship: e.target.value as RelationshipType }))}
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
              title="Relationship Type"
            >
              <option value="friend">Friend</option>
              <option value="acquaintance">Acquaintance</option>
              <option value="ring-of-trust">Ring of Trust</option>
              <option value="group-member">Group Member</option>
            </select>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
              Trust Level (1-10)
            </label>
            <input
              type="range"
              min="1"
              max="10"
              value={formData.trustLevel}
              onChange={(e) => setFormData(prev => ({ ...prev, trustLevel: parseInt(e.target.value) }))}
              className="w-full"
              title="Trust Level"
              placeholder="Trust Level"
            />
            <div className="text-center text-sm text-gray-500">{formData.trustLevel}</div>
          </div>
        </div>

        <div>
          <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
            Magnet URI *
          </label>
          <input
            type="text"
            value={formData.magnetUri}
            onChange={(e) => setFormData(prev => ({ ...prev, magnetUri: e.target.value }))}
            className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
            placeholder="magnet:?xt=urn:btih:..."
            required
          />
        </div>

        <div>
          <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
            Notes
          </label>
          <textarea
            value={formData.notes}
            onChange={(e) => setFormData(prev => ({ ...prev, notes: e.target.value }))}
            rows={3}
            className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
            placeholder="Optional notes about this contact..."
          />
        </div>

        <div className="flex gap-2 pt-4">
          <button
            type="submit"
            className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-md"
          >
            Add Contact
          </button>
          <button
            type="button"
            onClick={onCancel}
            className="px-4 py-2 bg-gray-300 hover:bg-gray-400 text-gray-700 rounded-md"
          >
            Cancel
          </button>
        </div>
      </form>
    </div>
  )
}

interface ContactCardProps {
  contact: Contact
  onUpdate: (contactId: string, updates: Partial<Contact>) => void
  onRemove: (contactId: string) => void
}

const ContactCard: React.FC<ContactCardProps> = ({ contact, onRemove }) => {
  const [isEditing, setIsEditing] = useState(false)
  const [storageLimit, setStorageLimit] = useState<number | null>(contact.storageLimitMB ?? null);
  const [savingLimit, setSavingLimit] = useState(false);

  const handleUpdate = () => {
    setIsEditing(!isEditing)
  }

  const handleStorageLimitChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const value = e.target.value === '' ? null : Math.max(0, parseInt(e.target.value));
    setStorageLimit(value);
  };

  const handleStorageLimitBlur = async () => {
    if (storageLimit === contact.storageLimitMB) return;
    setSavingLimit(true);
  const { updateContact } = await import('../stores/contactStore');
  await updateContact(contact.id, { storageLimitMB: storageLimit });
    setSavingLimit(false);
  };

  const getRelationshipColor = (relationship: RelationshipType) => {
    switch (relationship) {
      case 'ring-of-trust': return 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200'
      case 'friend': return 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200'
      case 'acquaintance': return 'bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200'
      case 'group-member': return 'bg-purple-100 text-purple-800 dark:bg-purple-900 dark:text-purple-200'
    }
  }

  const formatDate = (dateString: string) => {
    return new Date(dateString).toLocaleDateString()
  }

  return (
    <div className="bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg p-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center space-x-4">
          <div className="w-12 h-12 bg-gray-300 dark:bg-gray-600 rounded-full flex items-center justify-center">
            {contact.avatar ? (
              <img src={contact.avatar} alt={contact.displayName} className="w-12 h-12 rounded-full object-cover" />
            ) : (
              <span className="text-lg font-semibold text-gray-600 dark:text-gray-300">
                {contact.displayName.charAt(0).toUpperCase()}
              </span>
            )}
          </div>
          
          <div>
            <h3 className="font-semibold text-gray-900 dark:text-white">
              {contact.displayName}
            </h3>
            <p className="text-sm text-gray-500 dark:text-gray-400">
              @{contact.username}
            </p>
            <div className="flex items-center space-x-2 mt-1">
              <span className={`px-2 py-1 rounded-full text-xs font-medium ${getRelationshipColor(contact.relationship)}`}>
                {contact.relationship.replace('-', ' ')}
              </span>
              <span className="text-xs text-gray-500 dark:text-gray-400">
                Trust: {contact.trustLevel}/10
              </span>
            </div>
            <div className="flex items-center space-x-2 mt-2">
              <label className="text-xs text-gray-500 dark:text-gray-400" htmlFor={`storage-limit-${contact.id}`}>Storage Limit (MB):</label>
              <input
                id={`storage-limit-${contact.id}`}
                type="number"
                min={0}
                step={1}
                className="w-20 px-2 py-1 border border-gray-300 dark:border-gray-600 rounded text-xs bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
                value={storageLimit === null ? '' : storageLimit}
                onChange={handleStorageLimitChange}
                onBlur={handleStorageLimitBlur}
                disabled={savingLimit}
                placeholder="∞"
                title="Set max storage for this contact (MB)"
              />
              {savingLimit && <span className="text-xs text-blue-500 ml-1">Saving…</span>}
              {contact.storageUsed !== undefined && (
                <span className="text-xs text-gray-400 ml-2">Used: {Math.round((contact.storageUsed || 0) / 1024 / 1024)} MB</span>
              )}
            </div>
          </div>
        </div>

        <div className="flex items-center space-x-2">
          <button
            onClick={handleUpdate}
            className="px-3 py-1 text-sm bg-gray-100 hover:bg-gray-200 dark:bg-gray-700 dark:hover:bg-gray-600 text-gray-700 dark:text-gray-300 rounded"
          >
            Edit
          </button>
          <button
            onClick={() => onRemove(contact.id)}
            className="px-3 py-1 text-sm bg-red-100 hover:bg-red-200 dark:bg-red-900 dark:hover:bg-red-800 text-red-700 dark:text-red-300 rounded"
          >
            Remove
          </button>
        </div>
      </div>

      {contact.notes && (
        <div className="mt-3 p-2 bg-gray-50 dark:bg-gray-700 rounded text-sm text-gray-600 dark:text-gray-400">
          {contact.notes}
        </div>
      )}

      <div className="mt-3 text-xs text-gray-500 dark:text-gray-400">
        Added: {formatDate(contact.addedDate)}
        {contact.lastSeen && ` • Last seen: ${formatDate(contact.lastSeen)}`}
      </div>
    </div>
  )
}

interface EmptyStateProps {
  relationship: RelationshipType
  onAddContact: () => void
}

const EmptyState: React.FC<EmptyStateProps> = ({ relationship, onAddContact }) => {
  const getEmptyMessage = (relationship: RelationshipType) => {
    switch (relationship) {
      case 'friend': return 'No friends added yet. Start building your network!'
      case 'acquaintance': return 'No acquaintances yet. Add casual connections here.'
      case 'ring-of-trust': return 'No trusted contacts yet. Add your most trusted contacts for key recovery.'
      case 'group-member': return 'No group members yet. Add contacts from your groups.'
    }
  }

  return (
    <div className="text-center py-12">
      <div className="text-6xl mb-4">📱</div>
      <h3 className="text-lg font-medium text-gray-900 dark:text-white mb-2">
        {getEmptyMessage(relationship)}
      </h3>
      <p className="text-gray-500 dark:text-gray-400 mb-4">
        Add contacts to start building your decentralized social network.
      </p>
      <button
        onClick={onAddContact}
        className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg"
      >
        Add First Contact
      </button>
    </div>
  )
}

export default ContactsPage
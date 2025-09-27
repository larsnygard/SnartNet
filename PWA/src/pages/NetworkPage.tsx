import React, { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import QRCodeManager from '@/components/QRCodeManager'
import { useContactStore, type RelationshipType } from '@/stores/contactStore'

const relationshipTabs: Array<{ id: RelationshipType | 'all'; label: string; icon: string }> = [
  { id: 'all', label: 'All', icon: '🌐' },
  { id: 'friend', label: 'Friends', icon: '👥' },
  { id: 'ring-of-trust', label: 'Ring of Trust', icon: '🛡️' },
  { id: 'acquaintance', label: 'Acquaintances', icon: '🤝' },
  { id: 'group-member', label: 'Groups', icon: '🏠' },
]

const NetworkPage: React.FC = () => {
  const { loadContacts, contacts, addContactFromMagnet, removeContact } = useContactStore()
  const [activeTab, setActiveTab] = useState<'all' | RelationshipType>('all')
  const [magnetInput, setMagnetInput] = useState('')
  const [adding, setAdding] = useState(false)
  const [relationship, setRelationship] = useState<RelationshipType>('friend')
  const [error, setError] = useState<string | null>(null)

  useEffect(() => { loadContacts() }, [loadContacts])

  const filtered = contacts.filter(c => activeTab === 'all' ? true : c.relationship === activeTab)

  const handleAdd = async (e: React.FormEvent) => {
    e.preventDefault()
    setError(null)
    if (!magnetInput.trim()) return
    setAdding(true)
    try {
      const added = await addContactFromMagnet(magnetInput.trim(), relationship)
      if (!added) setError('Failed to load profile from magnet (no peers or invalid data)')
      else setMagnetInput('')
    } catch (e:any) {
      setError(e?.message || 'Failed to add contact')
    } finally {
      setAdding(false)
    }
  }

  return (
    <div className="max-w-6xl mx-auto p-6">
      <div className="mb-8">
        <div className="flex flex-col md:flex-row md:items-center md:justify-between gap-4 mb-6">
          <div>
            <h1 className="text-3xl font-bold text-gray-900 dark:text-white mb-2">Friends & Contacts</h1>
            <p className="text-gray-600 dark:text-gray-400 text-sm">Manage your network. Add new contacts with QR codes or magnet links.</p>
          </div>
        </div>
        
        {/* QR Code Section */}
        <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-6 mb-6">
          <h2 className="text-lg font-semibold text-gray-900 dark:text-white mb-4">Quick Connect</h2>
          <QRCodeManager onContactAdded={loadContacts} />
        </div>
        
        {/* Manual Magnet Link Form */}
        <div className="bg-gray-50 dark:bg-gray-800/50 rounded-lg border border-gray-200 dark:border-gray-700 p-4">
          <h3 className="text-md font-medium text-gray-700 dark:text-gray-300 mb-3">Or add via Magnet Link</h3>
          <form onSubmit={handleAdd} className="flex flex-col sm:flex-row gap-2 w-full md:w-auto">
          <input
            type="text"
            value={magnetInput}
            onChange={e => setMagnetInput(e.target.value)}
            placeholder="magnet:?xt=... (profile)"
            className="flex-1 px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-white text-sm"
            disabled={adding}
          />
          <select
            value={relationship}
            onChange={e => setRelationship(e.target.value as RelationshipType)}
            className="px-2 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-white text-sm"
            disabled={adding}
            aria-label="Relationship type"
          >
            <option value="friend">Friend</option>
            <option value="ring-of-trust">Ring of Trust</option>
            <option value="acquaintance">Acquaintance</option>
            <option value="group-member">Group Member</option>
          </select>
            <button
              type="submit"
              disabled={adding || !magnetInput.trim()}
              className="px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-gray-400 text-white rounded-md text-sm"
            >
              {adding ? 'Adding…' : 'Add'}
            </button>
          </form>
        </div>
      </div>

      {error && (
        <div className="mb-4 p-3 rounded bg-red-100 dark:bg-red-900/40 text-red-700 dark:text-red-300 text-sm">{error}</div>
      )}

      {/* Tabs */}
      <div className="border-b border-gray-200 dark:border-gray-700 mb-6 overflow-x-auto">
        <nav className="flex space-x-6 min-w-max">
          {relationshipTabs.map(tab => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id as any)}
              className={`py-2 px-1 border-b-2 text-sm font-medium transition-colors flex items-center gap-1 ${activeTab === tab.id ? 'border-blue-500 text-blue-600 dark:text-blue-400' : 'border-transparent text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-300'}`}
            >
              <span>{tab.icon}</span>{tab.label}
              <span className="ml-1 bg-gray-100 dark:bg-gray-700 text-gray-900 dark:text-gray-100 rounded-full px-2 py-0.5 text-[10px]">
                {tab.id === 'all' ? contacts.length : contacts.filter(c => c.relationship === tab.id).length}
              </span>
            </button>
          ))}
        </nav>
      </div>

      {/* List */}
      <div className="space-y-4">
        {filtered.length === 0 ? (
          <div className="text-center py-16 border border-dashed border-gray-300 dark:border-gray-700 rounded-lg">
            <div className="text-5xl mb-4">📭</div>
            <p className="text-gray-600 dark:text-gray-400 mb-2 text-sm">No contacts in this category yet.</p>
            <p className="text-xs text-gray-400 dark:text-gray-500">Add one with a profile magnet link above.</p>
          </div>
        ) : (
          filtered.map(c => (
            <div key={c.id} className="bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg p-4 flex items-center gap-4">
              <Link to={`/contact/${c.id}`} className="w-12 h-12 rounded-full bg-gradient-to-br from-blue-500 to-purple-600 flex items-center justify-center text-white font-semibold overflow-hidden focus:ring-2 focus:ring-blue-500">
                {c.avatar ? <img src={c.avatar} alt={c.displayName} className="w-12 h-12 rounded-full object-cover" /> : c.displayName.charAt(0).toUpperCase()}
              </Link>
              <div className="flex-1 min-w-0">
                <Link to={`/contact/${c.id}`} className="flex flex-wrap items-center gap-2 group">
                  <span className="font-medium text-gray-900 dark:text-white truncate group-hover:underline">{c.displayName}</span>
                  <span className="text-xs text-gray-500 dark:text-gray-400 truncate">@{c.username}</span>
                  <RelationshipBadge rel={c.relationship} />
                  {c.postIndexMagnetUri && <span className="text-[10px] px-2 py-0.5 rounded bg-indigo-100 dark:bg-indigo-900/40 text-indigo-700 dark:text-indigo-300">posts</span>}
                </Link>
                <div className="text-xs text-gray-400 dark:text-gray-500 mt-1">Trust {c.trustLevel}/10 • Added {new Date(c.addedDate).toLocaleDateString()}</div>
              </div>
              <button
                onClick={() => removeContact(c.id)}
                className="text-xs px-3 py-1 bg-red-100 hover:bg-red-200 dark:bg-red-900 dark:hover:bg-red-800 text-red-700 dark:text-red-300 rounded"
              >Remove</button>
            </div>
          ))
        )}
      </div>
    </div>
  )
}

const RelationshipBadge: React.FC<{ rel: RelationshipType }> = ({ rel }) => {
  const map: Record<RelationshipType, string> = {
    'friend': 'bg-green-100 dark:bg-green-900/40 text-green-700 dark:text-green-300',
    'ring-of-trust': 'bg-red-100 dark:bg-red-900/40 text-red-700 dark:text-red-300',
    'acquaintance': 'bg-blue-100 dark:bg-blue-900/40 text-blue-700 dark:text-blue-300',
    'group-member': 'bg-purple-100 dark:bg-purple-900/40 text-purple-700 dark:text-purple-300'
  }
  return <span className={`px-2 py-0.5 rounded-full text-[10px] font-medium ${map[rel]}`}>{rel.replace('-', ' ')}</span>
}

export default NetworkPage

import React, { useEffect, useState, useCallback } from 'react'
import { getFs, FsEntry } from '@/lib/fs'
import { listAppFs, readAppFs, writeAppFs } from '@/lib/appfs'

interface FileModalState { path: string; content: string }

// Modes:
//  - App Data (virtual overlay from domain stores via appfs.ts)
//  - Raw FS (actual persisted ZenFS/PFS backend)
// App Data mode is mostly read-only except editable profile.json and post content field.
const FilesPage: React.FC = () => {
  const [cwd, setCwd] = useState<string>('/')
  const [entries, setEntries] = useState<FsEntry[]>([])
  const [backend, setBackend] = useState<string>('')
  const [mode, setMode] = useState<'app' | 'raw'>('app')
  const [loading, setLoading] = useState<boolean>(true)
  const [error, setError] = useState<string>('')
  const [newFileName, setNewFileName] = useState('')
  const [newFolderName, setNewFolderName] = useState('')
  const [editModal, setEditModal] = useState<FileModalState | null>(null)
  const [fsReady, setFsReady] = useState(false)

  const refresh = useCallback(async (dir = cwd) => {
    setLoading(true)
    try {
      if (mode === 'app') {
        const list = await listAppFs(dir)
        setBackend('virtual-appfs')
        setEntries(list.map(e => ({ ...e } as any)))
      } else {
        const fs = await getFs()
        setBackend(fs.backend)
        const list = await fs.list(dir)
        // Hide system directories in root
        const filtered = dir === '/' ? list.filter(e => !['/etc','/tmp','/var','/dev','/proc'].includes(e.path)) : list
        setEntries(filtered)
      }
      setError('')
      setFsReady(true)
    } catch (e:any) {
      setError(e.message || String(e))
    } finally { setLoading(false) }
  }, [cwd, mode])

  useEffect(() => { refresh(cwd) }, [cwd, refresh])

  const open = async (entry: FsEntry) => {
    if (entry.type === 'dir') {
      setCwd(entry.path)
    } else {
      try {
        const content = mode === 'app' ? await readAppFs(entry.path) : await (await getFs()).readText(entry.path)
        setEditModal({ path: entry.path, content })
      } catch (e:any) { setError('Read failed: ' + (e.message || e)) }
    }
  }

  const up = () => {
    if (cwd === '/') return
    const parts = cwd.split('/').filter(Boolean)
    parts.pop()
    setCwd(parts.length ? '/' + parts.join('/') : '/')
  }

  const createFile = async () => {
    if (!newFileName.trim()) return
    const path = (cwd === '/' ? '' : cwd) + '/' + newFileName.trim()
    if (mode === 'app') {
      // Only allow creating editable top-level recognized files
      if (path !== '/profile.json') {
        setError('In App Data mode only profile.json can be created/edited directly')
        return
      }
      await writeAppFs('/profile.json', '{}')
    } else {
      const fs = await getFs(); await fs.writeText(path, '')
    }
    setNewFileName('')
    refresh()
  }

  const createFolder = async () => {
    if (!newFolderName.trim()) return
    const path = (cwd === '/' ? '' : cwd) + '/' + newFolderName.trim()
    if (mode === 'app') {
      setError('Cannot create folders in App Data mode')
      return
    }
    const fs = await getFs(); await fs.mkdir(path)
    setNewFolderName('')
    refresh()
  }

  const saveFile = async () => {
    if (!editModal) return
    if (mode === 'app') {
      try { await writeAppFs(editModal.path, editModal.content) } catch (e:any) { setError(e.message); return }
    } else { const fs = await getFs(); await fs.writeText(editModal.path, editModal.content) }
    setEditModal(null)
    refresh()
  }

  const deleteEntry = async (entry: FsEntry) => {
    if (!confirm(`Delete ${entry.path}?`)) return
    const fs = await getFs()
    try {
      if (entry.type === 'file') await fs.delete(entry.path)
      else {
        // naive recursive delete: list inside and delete files only (since fallback PFS has flat model)
        const stack = [entry.path]
        while (stack.length) {
          const d = stack.pop()!
          const list = await fs.list(d)
          for (const e of list) {
            if (e.type === 'dir') stack.push(e.path)
            else await fs.delete(e.path)
          }
        }
      }
      refresh()
    } catch (e:any) { setError('Delete failed: ' + (e.message || e)) }
  }

  const handleFileUpload = async (ev: React.ChangeEvent<HTMLInputElement>) => {
    const files = ev.target.files
    if (!files || !files.length) return
    if (mode === 'app') { setError('Uploads disabled in App Data mode'); return }
    const fs = await getFs()
    for (const f of Array.from(files)) { const text = await f.text(); const path = (cwd === '/' ? '' : cwd) + '/' + f.name; await fs.writeText(path, text) }
    ev.target.value = ''
    refresh()
  }

  return (
    <div className="p-4 space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">File Manager</h1>
        <span className="text-xs text-gray-500">Backend: {backend}{!fsReady && ' (initializing...)'}</span>
      </div>
      <div className="flex items-center space-x-3 text-xs">
        <label className="flex items-center space-x-1">
          <input type="radio" name="mode" value="app" checked={mode==='app'} onChange={()=>{setCwd('/'); setMode('app')}} />
          <span>App Data</span>
        </label>
        <label className="flex items-center space-x-1">
          <input type="radio" name="mode" value="raw" checked={mode==='raw'} onChange={()=>{setCwd('/'); setMode('raw')}} />
          <span>Raw FS</span>
        </label>
        {mode === 'app' && <span className="text-[10px] text-gray-500">Read-only except profile.json & post content edits</span>}
      </div>
      <div className="flex items-center space-x-2 text-sm">
        <button onClick={up} disabled={cwd === '/'} className="px-2 py-1 rounded bg-gray-200 dark:bg-gray-700 disabled:opacity-40">Up</button>
        <div className="font-mono text-xs break-all">{cwd}</div>
      </div>
      <div className="grid md:grid-cols-2 gap-4">
        <div className="space-y-3">
          <div className="flex space-x-2">
            <input placeholder="new-file.txt" value={newFileName} onChange={e=>setNewFileName(e.target.value)} className="flex-1 px-2 py-1 border rounded bg-transparent" />
            <button onClick={createFile} className="px-3 py-1 bg-blue-600 text-white rounded text-sm">Create File</button>
          </div>
          <div className="flex space-x-2">
            <input placeholder="new-folder" value={newFolderName} onChange={e=>setNewFolderName(e.target.value)} className="flex-1 px-2 py-1 border rounded bg-transparent" />
            <button onClick={createFolder} className="px-3 py-1 bg-indigo-600 text-white rounded text-sm">Create Folder</button>
          </div>
          <div className="flex items-center space-x-2">
            <label className="text-xs font-medium" htmlFor="fileUpload">Upload</label>
            <input id="fileUpload" aria-label="Upload files" type="file" multiple onChange={handleFileUpload} className="text-sm" />
          </div>
          {error && <div className="text-sm text-red-500">{error}</div>}
          <div className="border rounded overflow-auto max-h-[50vh]">
            <table className="w-full text-sm">
              <thead className="bg-gray-100 dark:bg-gray-800 sticky top-0">
                <tr>
                  <th className="text-left p-1">Name</th>
                  <th className="text-right p-1 w-24">Size</th>
                  <th className="p-1 w-28">Modified</th>
                  <th className="p-1 w-12"></th>
                </tr>
              </thead>
              <tbody>
                {loading && <tr><td colSpan={4} className="p-2 text-center text-xs">Loading...</td></tr>}
                {!loading && entries.length === 0 && <tr><td colSpan={4} className="p-4 text-center text-xs text-gray-500">Empty</td></tr>}
                {entries.map(e => {
                  const name = e.path === '/' ? '/' : e.path.split('/').filter(Boolean).slice(-1)[0]
                  return (
                    <tr key={e.path} className="hover:bg-gray-50 dark:hover:bg-gray-700/40 cursor-pointer">
                      <td className="p-1" onClick={()=>open(e)}>
                        <span className="font-mono">{e.type === 'dir' ? '📁 ' : '📄 '}{name}</span>
                      </td>
                      <td className="p-1 text-right tabular-nums">{e.type==='dir' ? '-' : e.size}</td>
                      <td className="p-1 text-[10px]">{new Date(e.mtime).toLocaleTimeString()}</td>
                      <td className="p-1 text-right">
                        <button onClick={()=>deleteEntry(e)} className="text-xs text-red-600 hover:underline">del</button>
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>
        </div>
        <div className="space-y-3">
          {editModal ? (
            <div className="flex flex-col h-full">
              <div className="flex items-center justify-between mb-2">
                <div className="font-mono text-xs break-all">{editModal.path}</div>
                <div className="space-x-2">
                  <button onClick={saveFile} className="px-2 py-1 bg-green-600 text-white rounded text-xs">Save</button>
                  <button onClick={()=>setEditModal(null)} className="px-2 py-1 bg-gray-300 dark:bg-gray-700 rounded text-xs">Close</button>
                </div>
              </div>
              <textarea aria-label="File contents" className="flex-1 w-full text-xs font-mono p-2 border rounded bg-transparent" value={editModal.content} onChange={e=>setEditModal({...editModal, content:e.target.value})} />
            </div>
          ) : (
            <div className="text-xs text-gray-500 border rounded p-4 h-full">
              Select a file to view / edit its contents.
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

export default FilesPage

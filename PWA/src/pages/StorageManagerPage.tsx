import { useEffect, useState } from 'react';
import { SnartStorage } from '../lib/SnartStorage';
import StorageFileManager from './StorageFileManager';

const storage = new SnartStorage();
const FILE_TYPES = ['posts', 'contacts', 'groups', 'keys', 'indexes'] as const;
type FileType = typeof FILE_TYPES[number];

export default function StorageManagerPage() {
  const [selectedType, setSelectedType] = useState<FileType>('posts');
  const [files, setFiles] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showImport] = useState(false);
  const [showExport, setShowExport] = useState(false);

  useEffect(() => {
    loadFiles(selectedType);
    // eslint-disable-next-line
  }, [selectedType]);

  async function loadFiles(type: FileType) {
    setLoading(true);
    setError(null);
    try {
      const list = await storage.listFiles('/' + type);
      setFiles(list);
    } catch (e: any) {
      setError(e.message || 'Failed to load files');
    }
    setLoading(false);
  }

  async function handleDelete(id: string) {
    if (!window.confirm('Delete this file?')) return;
    await storage.deleteFile(`/${selectedType}/${id}`);
    loadFiles(selectedType);
  }

  async function handleExport() {
    setShowExport(true);
    try {
      const zipBlob = await storage.exportAsZip();
      const url = URL.createObjectURL(zipBlob);
      const a = document.createElement('a');
      a.href = url;
      a.download = 'snartnet-storage.zip';
      document.body.appendChild(a);
      a.click();
      setTimeout(() => {
        document.body.removeChild(a);
        URL.revokeObjectURL(url);
      }, 100);
      alert('Export complete!');
    } catch (e: any) {
      alert('Export failed: ' + (e.message || e));
    }
    setShowExport(false);
  }

  // Import not implemented for Filer backend
  async function handleImport() {
    alert('Import is not available in this version.');
  }

  return (
    <div className="max-w-3xl mx-auto p-4">
      <h1 className="text-2xl font-bold mb-4">Storage Manager</h1>
      <div className="mb-6">
        <StorageFileManager />
      </div>
      <div className="flex gap-2 mb-4">
        {FILE_TYPES.map((type) => (
          <button
            key={type}
            className={`px-3 py-1 rounded ${selectedType === type ? 'bg-blue-600 text-white' : 'bg-gray-200'}`}
            onClick={() => setSelectedType(type)}
          >
            {type}
          </button>
        ))}
      </div>
      <div className="flex gap-2 mb-4">
        <button
          className="bg-green-600 text-white px-3 py-1 rounded"
          onClick={handleExport}
          disabled={showExport}
        >
          Export All
        </button>
        <button
          className="bg-yellow-600 text-white px-3 py-1 rounded"
          onClick={handleImport}
          disabled={showImport}
        >
          Import All
        </button>
      </div>
      {loading ? (
        <div>Loading...</div>
      ) : error ? (
        <div className="text-red-600">{error}</div>
      ) : (
        <table className="w-full border mt-2">
          <thead>
            <tr className="bg-gray-100">
              <th className="p-2 text-left">File Name</th>
              <th className="p-2 text-left">Actions</th>
            </tr>
          </thead>
          <tbody>
            {files.length === 0 ? (
              <tr><td colSpan={2} className="text-center p-4">No files</td></tr>
            ) : files.map((file) => (
              <tr key={file} className="border-t">
                <td className="p-2 font-mono">{file}</td>
                <td className="p-2">
                  <button
                    className="bg-red-600 text-white px-2 py-1 rounded"
                    onClick={() => handleDelete(file)}
                  >
                    Delete
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

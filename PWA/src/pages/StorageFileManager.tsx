import { useEffect, useState } from 'react';
import { SnartStorage } from '../lib/SnartStorage';

const storage = new SnartStorage();

function FileNode({ path, name, isDir, onOpen, onDelete }: {
  path: string;
  name: string;
  isDir: boolean;
  onOpen: (path: string) => void;
  onDelete: (path: string) => void;
}) {
  return (
    <div className="flex items-center gap-2 pl-2 py-0.5 group hover:bg-gray-100 dark:hover:bg-gray-800 rounded">
      {isDir ? (
        <span className="font-bold text-blue-600 cursor-pointer" onClick={() => onOpen(path)}>
          📁 {name}
        </span>
      ) : (
        <span className="font-mono">📄 {name}</span>
      )}
      {!isDir && (
        <button
          className="ml-auto text-xs text-red-600 hover:underline opacity-0 group-hover:opacity-100"
          onClick={() => onDelete(path)}
        >
          Delete
        </button>
      )}
    </div>
  );
}

function FileTree({ dir, onDelete }: { dir: string; onDelete: (path: string) => void }) {
  const [entries, setEntries] = useState<{ name: string; isDir: boolean }[]>([]);
  const [expanded, setExpanded] = useState<{ [name: string]: boolean }>({});

  useEffect(() => {
    let mounted = true;
    storage.listFiles(dir).then(async (files) => {
      const result = await Promise.all(
        files.map(async (name) => {
          const path = dir.endsWith('/') ? dir + name : dir + '/' + name;
          const isDir = await storage.fileExists(path + '/');
          return { name, isDir };
        })
      );
      if (mounted) setEntries(result);
    });
    return () => { mounted = false; };
  }, [dir]);

  const handleOpen = (name: string) => {
    setExpanded((prev) => ({ ...prev, [name]: !prev[name] }));
  };

  return (
    <div className="pl-2">
      {entries.map(({ name, isDir }) => {
        const path = dir.endsWith('/') ? dir + name : dir + '/' + name;
        return (
          <div key={path}>
            <FileNode
              path={path}
              name={name}
              isDir={isDir}
              onOpen={() => handleOpen(name)}
              onDelete={onDelete}
            />
            {isDir && expanded[name] && (
              <FileTree dir={path} onDelete={onDelete} />
            )}
          </div>
        );
      })}
    </div>
  );
}

export default function StorageFileManager() {
  const [root] = useState('/');
  const [refresh, setRefresh] = useState(0);

  const handleDelete = async (path: string) => {
    if (!window.confirm(`Delete file ${path}?`)) return;
    await storage.deleteFile(path);
    setRefresh((r) => r + 1);
  };

  // re-render on refresh
  useEffect(() => {}, [refresh]);

  return (
    <div className="bg-white dark:bg-gray-900 rounded shadow p-4">
      <h2 className="text-lg font-semibold mb-2">File Manager</h2>
      <FileTree dir={root} onDelete={handleDelete} />
    </div>
  );
}

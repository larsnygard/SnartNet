import React, { useState, useMemo } from 'react';
import getTorrentService from '../lib/torrent';
import { useProfileStore } from '../stores/profileStore';
import { usePostStore } from '../stores/postStore';

// Reworked ProfilePosts: now uses global postStore (signed & seeded posts)
// instead of the removed mock profileStore post methods.
const ProfilePosts: React.FC = () => {
  const { currentProfile } = useProfileStore();
  const { posts, addPost, deletePost, editPost } = usePostStore() as any;
  const [postContent, setPostContent] = useState('');
  const [posting, setPosting] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editContent, setEditContent] = useState('');

  const myPosts = useMemo(() => {
    if (!currentProfile) return [] as any[];
    return posts
      .filter((p: any) => p.author === currentProfile.username)
      .sort((a: any, b: any) => new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime());
  }, [posts, currentProfile]);

  if (!currentProfile) return null;

  const handlePost = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!postContent.trim()) return;
    setPosting(true);
    try {
      await addPost({
        author: currentProfile.username,
        authorDisplayName: currentProfile.displayName,
        content: postContent.trim(),
        images: []
      } as any);
      setPostContent('');
    } finally {
      setPosting(false);
    }
  };

  // Download post file from torrent and trigger browser download
  const handleDownloadPostFile = async (magnetUri: string, filename = 'post.json') => {
    try {
      const svc = getTorrentService();
      const client = (svc as any).client;
      if (!client) throw new Error('WebTorrent client not initialized');
      const torrent = client.get(magnetUri) || client.add(magnetUri);
      torrent.on('done', () => {
        const file = torrent.files.find((f: any) => f.name && f.name.startsWith('post_') && f.name.endsWith('.json'));
        if (!file) return alert('Post file not found in torrent');
        file.getBlob((err: any, blob: Blob) => {
          if (err) return alert('Failed to get file: ' + err.message);
          const url = URL.createObjectURL(blob);
          const a = document.createElement('a');
          a.href = url;
          a.download = file.name || filename;
          document.body.appendChild(a);
          a.click();
          setTimeout(() => {
            document.body.removeChild(a);
            URL.revokeObjectURL(url);
          }, 100);
        });
      });
    } catch (e: any) {
      alert('Download failed: ' + (e?.message || e));
    }
  };

  return (
    <div className="w-full max-w-xl mx-auto mt-8">
      <form onSubmit={handlePost} className="mb-6 flex flex-col gap-2">
        <textarea
          className="w-full p-3 border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white resize-vertical"
          rows={3}
          placeholder="What's on your mind?"
          value={postContent}
          onChange={e => setPostContent(e.target.value)}
          disabled={posting}
        />
        <div className="flex items-center justify-between">
          <span className="text-xs text-gray-500 dark:text-gray-400">
            {postContent.length > 0 && `${postContent.length} chars`}
          </span>
          <button
            type="submit"
            className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg disabled:bg-gray-400"
            disabled={posting || !postContent.trim()}
          >
            {posting ? 'Posting...' : 'Post'}
          </button>
        </div>
      </form>
      <div className="space-y-4">
        {myPosts.length > 0 ? (
          myPosts.map((post: any) => (
            <div key={post.id} className="bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg p-4 relative">
              <p className="text-gray-900 dark:text-gray-100 mb-2 whitespace-pre-line">{post.content}</p>
              <div className="text-xs text-gray-500 dark:text-gray-400 flex flex-wrap gap-2 justify-between items-center">
                <span>{new Date(post.createdAt).toLocaleString()}</span>
                <div className="flex items-center gap-2">
                  {post.signature && (
                    <span
                      className={
                        'inline-flex items-center px-2 py-0.5 rounded text-[10px] font-semibold ' +
                        (post.signatureVerified ? 'bg-green-100 text-green-700 dark:bg-green-900 dark:text-green-300' : 'bg-red-100 text-red-700 dark:bg-red-900 dark:text-red-300')
                      }
                      title={post.signatureVerified ? `Signature verified${post.fingerprint ? ' (' + post.fingerprint + ')' : ''}` : `Signature invalid: ${post.signatureError || 'unknown'}`}
                    >
                      {post.signatureVerified ? 'Verified' : 'Invalid'}
                    </span>
                  )}
                  {post.magnetUri && (
                    <button
                      className="text-green-600 hover:text-green-800 text-xs border border-green-200 dark:border-green-700 rounded px-2 py-0.5 ml-2"
                      onClick={() => handleDownloadPostFile(post.magnetUri, `post_${post.id}.json`)}
                      title="Download post file from torrent"
                    >
                      ⬇️ Download
                    </button>
                  )}
                  <button
                    className="text-blue-500 hover:text-blue-700 text-xs"
                    onClick={() => { setEditingId(post.id); setEditContent(post.content) }}
                    title="Edit post"
                  >
                    ✏️
                  </button>
                  <button
                    className="text-red-500 hover:text-red-700 text-xs"
                    onClick={() => deletePost(post.id)}
                    title="Delete post"
                  >
                    🗑️
                  </button>
                </div>
              </div>
              {editingId === post.id && (
                <div className="mt-2 space-y-2">
                  <textarea
                    className="w-full p-2 border border-gray-300 dark:border-gray-700 rounded bg-white dark:bg-gray-900 text-sm"
                    value={editContent}
                    onChange={e => setEditContent(e.target.value)}
                    placeholder="Edit your post"
                    title="Edit post content"
                  />
                  <div className="flex gap-2 justify-end">
                    <button
                      className="px-3 py-1 text-xs rounded bg-gray-300 dark:bg-gray-700 hover:bg-gray-400 dark:hover:bg-gray-600"
                      onClick={() => { setEditingId(null); setEditContent('') }}
                    >
                      Cancel
                    </button>
                    <button
                      className="px-3 py-1 text-xs rounded bg-green-600 hover:bg-green-700 text-white"
                      disabled={!editContent.trim()}
                      onClick={async () => { await editPost(post.id, editContent.trim()); setEditingId(null); setEditContent('') }}
                    >
                      Save
                    </button>
                  </div>
                </div>
              )}
            </div>
          ))
        ) : (
          <div className="text-center text-gray-500 dark:text-gray-400">No posts yet.</div>
        )}
      </div>
    </div>
  );
};

export default ProfilePosts;

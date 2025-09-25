import React, { useState } from 'react';
import { useProfileStore, ProfilePost } from '../stores/profileStore';

const ProfilePosts: React.FC = () => {
  const { currentProfile, addProfilePost, removeProfilePost } = useProfileStore();
  const [postContent, setPostContent] = useState('');
  const [posting, setPosting] = useState(false);

  if (!currentProfile) return null;

  const handlePost = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!postContent.trim()) return;
    setPosting(true);
    addProfilePost(currentProfile.username, postContent.trim());
    setPostContent('');
    setPosting(false);
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
        <button
          type="submit"
          className="self-end px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg disabled:bg-gray-400"
          disabled={posting || !postContent.trim()}
        >
          {posting ? 'Posting...' : 'Post'}
        </button>
      </form>
      <div className="space-y-4">
        {currentProfile.posts && currentProfile.posts.length > 0 ? (
          currentProfile.posts.map((post: ProfilePost) => (
            <div key={post.id} className="bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg p-4 relative">
              <p className="text-gray-900 dark:text-gray-100 mb-2 whitespace-pre-line">{post.content}</p>
              <div className="text-xs text-gray-500 dark:text-gray-400 flex justify-between items-center">
                <span>{new Date(post.createdAt).toLocaleString()}</span>
                <button
                  className="text-red-500 hover:text-red-700 text-xs ml-2"
                  onClick={() => removeProfilePost(currentProfile.username, post.id)}
                  title="Delete post"
                >
                  🗑️
                </button>
              </div>
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

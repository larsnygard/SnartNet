
export interface TorrentPost {
  id: string;
  author: string;
  content: string;
  createdAt: string;
  signature?: string;
  authorPublicKey?: string;
  magnetUri?: string;
  images?: PostImage[];
  tags?: string[];
  signatureVerified?: boolean;
  signatureError?: string;
  authorDisplayName?: string;
  fingerprint?: string;
  isSeeding?: boolean;
  seedProgress?: number;
  seedError?: string;
}
import { create } from 'zustand';
import { signPost } from '@/lib/crypto/ed25519';
import { SnartStorage } from '../lib/SnartStorage';

export interface PostImage {
  id: string;
  data: string; // base64 data
  filename: string;
  size: number;
  mimeType: string;
}

interface PostState {
  posts: TorrentPost[];
  loading: boolean;
  error: string | null;
  setPosts: (posts: TorrentPost[]) => void;
  addPost?: (postData: Omit<TorrentPost, 'id' | 'createdAt' | 'magnetUri'>) => Promise<void>;
  loadPosts?: () => Promise<void>;
}

// Async helpers for file IO
export const POSTS_DIR = '/posts';
const storage = new SnartStorage();

export async function loadPostsFromFS(): Promise<TorrentPost[]> {
  let files: string[] = [];
  try {
    files = await storage.listFiles(POSTS_DIR);
  } catch (e) {
    // Directory may not exist yet
    return [];
  }
  const posts: TorrentPost[] = [];
  for (const file of files) {
    try {
      const data = await storage.readFile(`${POSTS_DIR}/${file}`);
      if (data) posts.push(JSON.parse(data));
    } catch (e) { console.warn('Failed to load post', file, e); }
  }
  return posts;
}

export async function savePostToFS(post: TorrentPost) {
  await storage.writeFile(`${POSTS_DIR}/${post.id}.json`, JSON.stringify(post));
}

export async function removePostFromFS(postId: string) {
  await storage.deleteFile(`${POSTS_DIR}/${postId}.json`);
}

// const LOCAL_STORAGE_KEY = 'snartnet:posts';

export const usePostStore = create<PostState>((set) => ({
  posts: [],
  loading: false,
  error: null,
  setPosts: (posts) => set({ posts }),
  addPost: async (postData) => {
    await addPost(postData);
  },
  loadPosts: async () => {
    await loadPosts();
  },
}));

// Async helpers for actions
export async function addPost(postData: Omit<TorrentPost, 'id' | 'createdAt' | 'magnetUri'>) {
  const createdAt = new Date().toISOString();
  let signature: string | undefined;
  let authorPublicKey: string | undefined;
  try {
    const signed = await signPost({
      version: 1,
      kind: 'post',
      body: postData.content,
      createdAt,
      attachments: postData.images?.map(i => i.id),
      // parentId, replyTo can be added if present in postData
    });
    signature = signed.signature;
    authorPublicKey = signed.authorPublicKey;
  } catch (e) {
    signature = undefined;
    authorPublicKey = undefined;
  }
  const post: TorrentPost = {
    ...postData,
    id: Math.random().toString(36).slice(2),
    createdAt,
    signature,
    authorPublicKey,
  };
  await savePostToFS(post);
  const posts = await loadPostsFromFS();
  usePostStore.getState().setPosts(posts);
}

export async function updatePost(postId: string, updates: Partial<TorrentPost>) {
  const posts = await loadPostsFromFS();
  const idx = posts.findIndex(p => p.id === postId);
  if (idx >= 0) {
    const updated = { ...posts[idx], ...updates };
    await savePostToFS(updated);
    posts[idx] = updated;
    usePostStore.getState().setPosts(posts);
  }
}

export async function removePost(postId: string) {
  await removePostFromFS(postId);
  const posts = (await loadPostsFromFS()).filter(p => p.id !== postId);
  usePostStore.getState().setPosts(posts);
}

export async function loadPosts() {
  const posts = await loadPostsFromFS();
  usePostStore.getState().setPosts(posts);
}

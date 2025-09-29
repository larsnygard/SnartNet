// SnartStorage.ts
// Modular file storage backend using Filer (IndexedDB-backed virtual filesystem)
// https://github.com/filerjs/filer

import Filer from 'filer';

export class SnartStorage {
  private fs: any;
  private ready: Promise<void>;

  constructor() {
    this.fs = new Filer.FileSystem({
      name: 'SnartNetFS',
      flags: { persistent: true },
      provider: new Filer.FileSystem.providers.IndexedDB()
    });
    this.ready = new Promise((resolve, reject) => {
      this.fs.init({
        name: 'SnartNetFS',
        flags: { persistent: true },
        provider: new Filer.FileSystem.providers.IndexedDB()
      }, (err: any) => {
        if (err) reject(err); else resolve();
      });
    });
  }

  // Write a file (string or Blob)
  async writeFile(path: string, data: string | Blob): Promise<void> {
    await this.ready;
    return new Promise((resolve, reject) => {
      if (typeof data === 'string') {
        this.fs.writeFile(path, data, 'utf8', (err: any) => {
          if (err) reject(err); else resolve();
        });
      } else {
        // Blob: convert to ArrayBuffer
        const reader = new FileReader();
        reader.onload = () => {
          const buffer = new Uint8Array(reader.result as ArrayBuffer);
          this.fs.writeFile(path, buffer, (err: any) => {
            if (err) reject(err); else resolve();
          });
        };
        reader.onerror = reject;
        reader.readAsArrayBuffer(data);
      }
    });
  }

  // Read a file as string
  async readFile(path: string): Promise<string> {
    await this.ready;
    return new Promise((resolve, reject) => {
      this.fs.readFile(path, 'utf8', (err: any, data: string) => {
        if (err) reject(err); else resolve(data);
      });
    });
  }

  // Delete a file
  async deleteFile(path: string): Promise<void> {
    await this.ready;
    return new Promise((resolve, reject) => {
      this.fs.unlink(path, (err: any) => {
        if (err) reject(err); else resolve();
      });
    });
  }

  // List files in a directory
  async listFiles(dir: string): Promise<string[]> {
    await this.ready;
    return new Promise((resolve, reject) => {
      this.fs.readdir(dir, (err: any, files: string[]) => {
        if (err) reject(err); else resolve(files);
      });
    });
  }

  // Check if a file exists
  async fileExists(path: string): Promise<boolean> {
    await this.ready;
    return new Promise((resolve) => {
      this.fs.stat(path, (err: any) => {
        resolve(!err);
      });
    });
  }

  // Bonus: Export the entire FS as a ZIP file
  async exportAsZip(): Promise<Blob> {
    await this.ready;
    // Dynamically import JSZip for zipping
    const JSZip = (await import('jszip')).default;
    const zip = new JSZip();
    await this._addDirToZip('/', zip);
    return zip.generateAsync({ type: 'blob' });
  }

  // Recursively add files to zip
  private async _addDirToZip(dir: string, zip: any, root = ''): Promise<void> {
    const files = await this.listFiles(dir);
    for (const file of files) {
      const fullPath = dir.endsWith('/') ? dir + file : dir + '/' + file;
      const stat = await new Promise<any>((resolve) => this.fs.stat(fullPath, (err: any, s: any) => resolve(err ? null : s)));
      if (!stat) continue;
      if (stat.isDirectory()) {
        const folder = zip.folder(file);
        await this._addDirToZip(fullPath, folder, root + file + '/');
      } else {
        const content = await new Promise<any>((resolve, reject) => this.fs.readFile(fullPath, (err: any, data: any) => err ? reject(err) : resolve(data)));
        zip.file(root + file, content);
      }
    }
  }
}

/*
Usage Example:

import { SnartStorage } from './SnartStorage';
const storage = new SnartStorage();

// Write a post
await storage.writeFile('/posts/123.json', JSON.stringify({ text: 'Hello world' }));

// Read a post
const post = await storage.readFile('/posts/123.json');

// List all posts
const files = await storage.listFiles('/posts');

// Check if a file exists
const exists = await storage.fileExists('/posts/123.json');

// Delete a file
await storage.deleteFile('/posts/123.json');

// Export as ZIP
const zipBlob = await storage.exportAsZip();
// Download: URL.createObjectURL(zipBlob)
*/

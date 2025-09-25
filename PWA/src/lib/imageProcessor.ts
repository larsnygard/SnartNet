/**
 * Image processing utilities for profile pictures
 * Handles upload, resize, crop, and optimization
 */

export interface ProcessedImage {
  dataUrl: string
  blob: Blob
  width: number
  height: number
  size: number
  format: string
}

export interface CropArea {
  x: number
  y: number
  width: number
  height: number
}

export class ImageProcessor {
  // Standard profile picture sizes
  static readonly PROFILE_SIZE = 200 // 200x200px standard
  static readonly THUMBNAIL_SIZE = 64 // 64x64px thumbnail
  static readonly MAX_FILE_SIZE = 2 * 1024 * 1024 // 2MB max
  static readonly SUPPORTED_FORMATS = ['image/jpeg', 'image/png', 'image/webp']
  static readonly OUTPUT_FORMAT = 'image/jpeg'
  static readonly OUTPUT_QUALITY = 0.85

  /**
   * Process uploaded file into profile picture
   */
  static async processProfilePicture(file: File): Promise<ProcessedImage> {
    // Validate file
    this.validateFile(file)

    // Load image
    const img = await this.loadImage(file)
    
    // Auto-crop to square (largest centered square)
    const cropArea = this.calculateSquareCrop(img.width, img.height)
    
    // Resize and compress
    const processed = await this.resizeAndCompress(img, cropArea, this.PROFILE_SIZE)
    
    return processed
  }

  /**
   * Create thumbnail from processed image
   */
  static async createThumbnail(processedImage: ProcessedImage): Promise<ProcessedImage> {
    const img = await this.loadImageFromDataUrl(processedImage.dataUrl)
    const cropArea = { x: 0, y: 0, width: img.width, height: img.height }
    return this.resizeAndCompress(img, cropArea, this.THUMBNAIL_SIZE)
  }

  /**
   * Process image with custom crop area
   */
  static async processWithCrop(file: File, cropArea: CropArea): Promise<ProcessedImage> {
    this.validateFile(file)
    const img = await this.loadImage(file)
    return this.resizeAndCompress(img, cropArea, this.PROFILE_SIZE)
  }

  /**
   * Validate uploaded file
   */
  private static validateFile(file: File): void {
    if (!this.SUPPORTED_FORMATS.includes(file.type)) {
      throw new Error(`Unsupported file format. Please use: ${this.SUPPORTED_FORMATS.join(', ')}`)
    }

    if (file.size > this.MAX_FILE_SIZE) {
      throw new Error(`File too large. Maximum size: ${Math.round(this.MAX_FILE_SIZE / 1024 / 1024)}MB`)
    }
  }

  /**
   * Load image from file
   */
  private static loadImage(file: File): Promise<HTMLImageElement> {
    return new Promise((resolve, reject) => {
      const img = new Image()
      const url = URL.createObjectURL(file)
      
      img.onload = () => {
        URL.revokeObjectURL(url)
        resolve(img)
      }
      
      img.onerror = () => {
        URL.revokeObjectURL(url)
        reject(new Error('Failed to load image'))
      }
      
      img.src = url
    })
  }

  /**
   * Load image from data URL
   */
  private static loadImageFromDataUrl(dataUrl: string): Promise<HTMLImageElement> {
    return new Promise((resolve, reject) => {
      const img = new Image()
      
      img.onload = () => resolve(img)
      img.onerror = () => reject(new Error('Failed to load image from data URL'))
      
      img.src = dataUrl
    })
  }

  /**
   * Calculate the largest centered square crop area
   */
  private static calculateSquareCrop(width: number, height: number): CropArea {
    const size = Math.min(width, height)
    const x = (width - size) / 2
    const y = (height - size) / 2
    
    return { x, y, width: size, height: size }
  }

  /**
   * Resize image and compress
   */
  private static async resizeAndCompress(
    img: HTMLImageElement, 
    cropArea: CropArea, 
    targetSize: number
  ): Promise<ProcessedImage> {
    const canvas = document.createElement('canvas')
    const ctx = canvas.getContext('2d')
    
    if (!ctx) {
      throw new Error('Canvas context not available')
    }

    // Set canvas size to target dimensions
    canvas.width = targetSize
    canvas.height = targetSize

    // Enable image smoothing for better quality
    ctx.imageSmoothingEnabled = true
    ctx.imageSmoothingQuality = 'high'

    // Draw cropped and resized image
    ctx.drawImage(
      img,
      cropArea.x, cropArea.y, cropArea.width, cropArea.height, // Source
      0, 0, targetSize, targetSize // Destination
    )

    // Convert to blob
    const blob = await new Promise<Blob>((resolve, reject) => {
      canvas.toBlob(
        (blob) => {
          if (blob) {
            resolve(blob)
          } else {
            reject(new Error('Failed to create blob'))
          }
        },
        this.OUTPUT_FORMAT,
        this.OUTPUT_QUALITY
      )
    })

    // Get data URL
    const dataUrl = canvas.toDataURL(this.OUTPUT_FORMAT, this.OUTPUT_QUALITY)

    return {
      dataUrl,
      blob,
      width: targetSize,
      height: targetSize,
      size: blob.size,
      format: this.OUTPUT_FORMAT
    }
  }

  /**
   * Convert blob to base64 string for storage
   */
  static async blobToBase64(blob: Blob): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader()
      reader.onload = () => {
        const result = reader.result as string
        // Remove data URL prefix to get just the base64 data
        const base64 = result.split(',')[1]
        resolve(base64)
      }
      reader.onerror = () => reject(new Error('Failed to convert blob to base64'))
      reader.readAsDataURL(blob)
    })
  }

  /**
   * Convert base64 string back to data URL
   */
  static base64ToDataUrl(base64: string, format: string = this.OUTPUT_FORMAT): string {
    return `data:${format};base64,${base64}`
  }

  /**
   * Get formatted file size string
   */
  static formatFileSize(bytes: number): string {
    if (bytes === 0) return '0 B'
    
    const k = 1024
    const sizes = ['B', 'KB', 'MB']
    const i = Math.floor(Math.log(bytes) / Math.log(k))
    
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i]
  }

  /**
   * Check if image dimensions are suitable for profile picture
   */
  static validateImageDimensions(width: number, height: number): void {
    const minSize = 100
    const maxSize = 4000
    
    if (width < minSize || height < minSize) {
      throw new Error(`Image too small. Minimum size: ${minSize}x${minSize}px`)
    }
    
    if (width > maxSize || height > maxSize) {
      throw new Error(`Image too large. Maximum size: ${maxSize}x${maxSize}px`)
    }
  }

  /**
   * Generate avatar initials as fallback
   */
  static generateAvatarInitials(name: string): string {
    return name
      .split(' ')
      .map(word => word.charAt(0).toUpperCase())
      .join('')
      .substring(0, 2)
  }

  /**
   * Generate gradient avatar background
   */
  static generateAvatarGradient(seed: string): string {
    // Simple hash function for consistent colors
    let hash = 0
    for (let i = 0; i < seed.length; i++) {
      const char = seed.charCodeAt(i)
      hash = ((hash << 5) - hash) + char
      hash = hash & hash // Convert to 32-bit integer
    }
    
    // Generate two colors based on hash
    const hue1 = Math.abs(hash % 360)
    const hue2 = (hue1 + 60) % 360
    
    return `linear-gradient(135deg, hsl(${hue1}, 70%, 50%), hsl(${hue2}, 70%, 70%))`
  }
}

export async function processAndStoreImage(file: File, _type: 'post-image' | 'avatar'): Promise<{ id: string, data: string, filename: string, size: number, mimeType: string } | null> {
  // For now, we just convert to base64. We can add resizing later.
  if (!file.type.startsWith('image/')) {
    console.warn(`File is not an image: ${file.name}`);
    return null;
  }

  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = (e) => {
      const data = e.target?.result as string;
      if (data) {
        resolve({
          id: `${file.name}-${file.lastModified}`,
          data,
          filename: file.name,
          size: file.size,
          mimeType: file.type,
        });
      } else {
        reject(new Error('Failed to read file data.'));
      }
    };
    reader.onerror = (error) => {
      reject(error);
    };
    reader.readAsDataURL(file);
  });
}
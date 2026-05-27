import React, { useState, useEffect, useRef } from 'react'
import QRCode from 'qrcode'
import QrScanner from 'qr-scanner'
import { useProfileStore } from '@/stores/profileStore'
import { useContactStore } from '@/stores/contactStore'
import { getCore } from '@/lib/core'

interface QRCodeManagerProps {
  onContactAdded?: (contact: any) => void
}

export const QRCodeManager: React.FC<QRCodeManagerProps> = ({ onContactAdded }) => {
  const [showQR, setShowQR] = useState(false)
  const [showScanner, setShowScanner] = useState(false)
  const [qrDataURL, setQrDataURL] = useState<string>('')
  const [scanResult, setScanResult] = useState<string>('')
  const videoRef = useRef<HTMLVideoElement>(null)
  const scannerRef = useRef<QrScanner | null>(null)

  const { currentProfile } = useProfileStore()
  const { addContactFromMagnet } = useContactStore()

  // Generate QR code for current profile
  useEffect(() => {
    const generateQRCode = async () => {
      if (showQR && currentProfile) {
        let magnetUri = currentProfile.magnetUri
        
        // If no magnetUri is available, try to generate one by seeding the profile
        if (!magnetUri && currentProfile.username) {
          try {
            const core = await getCore()
            magnetUri = await core.seedCurrentProfile()
            console.log('[QRCodeManager] Generated magnet URI for QR code:', magnetUri)
          } catch (error) {
            console.error('[QRCodeManager] Failed to generate magnet URI:', error)
            return
          }
        }
        
        if (magnetUri) {
          const profileData = {
            username: currentProfile.username,
            displayName: currentProfile.displayName || currentProfile.username,
            magnetUri: magnetUri,
            type: 'snartnet-profile'
          }
          
          QRCode.toDataURL(JSON.stringify(profileData), {
            width: 256,
            margin: 2,
            color: {
              dark: '#000000',
              light: '#ffffff'
            }
          }).then(setQrDataURL).catch(console.error)
        }
      }
    }
    
    generateQRCode()
  }, [showQR, currentProfile])

  // Initialize QR scanner
  useEffect(() => {
    if (showScanner && videoRef.current) {
      scannerRef.current = new QrScanner(
        videoRef.current,
        (result) => {
          setScanResult(result.data)
          handleScanResult(result.data)
        },
        {
          highlightScanRegion: true,
          highlightCodeOutline: true,
        }
      )
      
      scannerRef.current.start().catch(console.error)
      
      return () => {
        if (scannerRef.current) {
          scannerRef.current.stop()
          scannerRef.current.destroy()
          scannerRef.current = null
        }
      }
    }
  }, [showScanner])

  const handleScanResult = async (data: string) => {
    try {
      // Try to parse as SnartNet profile JSON
      const profileData = JSON.parse(data)
      if (profileData.type === 'snartnet-profile' && profileData.magnetUri) {
        const contact = await addContactFromMagnet(profileData.magnetUri, 'friend')
        if (contact) {
          setScanResult(`Added ${profileData.username} as a friend!`)
          onContactAdded?.(contact)
          setShowScanner(false)
        } else {
          setScanResult('Failed to add contact - profile may be invalid')
        }
      } else {
        // Fallback: try as direct magnet URI
        if (data.startsWith('magnet:')) {
          const contact = await addContactFromMagnet(data, 'friend')
          if (contact) {
            setScanResult(`Added contact from magnet URI!`)
            onContactAdded?.(contact)
            setShowScanner(false)
          } else {
            setScanResult('Failed to add contact from magnet URI')
          }
        } else {
          setScanResult('Invalid QR code - not a SnartNet profile or magnet link')
        }
      }
    } catch (error) {
      // Not valid JSON, try as magnet URI
      if (data.startsWith('magnet:')) {
        const contact = await addContactFromMagnet(data, 'friend')
        if (contact) {
          setScanResult(`Added contact from magnet URI!`)
          onContactAdded?.(contact)
          setShowScanner(false)
        } else {
          setScanResult('Failed to add contact from magnet URI')
        }
      } else {
        setScanResult('Invalid QR code format')
      }
    }
  }

  const closeQR = () => {
    setShowQR(false)
    setQrDataURL('')
  }

  const closeScanner = () => {
    setShowScanner(false)
    setScanResult('')
    if (scannerRef.current) {
      scannerRef.current.stop()
      scannerRef.current.destroy()
      scannerRef.current = null
    }
  }

  return (
    <div className="space-y-4">
      <div className="flex gap-4">
        <button
          onClick={() => setShowQR(true)}
          disabled={!currentProfile?.username}
          className="px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-gray-400 text-white rounded-lg transition-colors flex items-center gap-2"
        >
          📱 Show My QR Code
        </button>
        
        <button
          onClick={() => setShowScanner(true)}
          className="px-4 py-2 bg-green-600 hover:bg-green-700 text-white rounded-lg transition-colors flex items-center gap-2"
        >
          📷 Scan QR Code
        </button>
      </div>

      {/* QR Code Display Modal */}
      {showQR && (
        <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
          <div className="bg-white dark:bg-gray-800 p-6 rounded-lg max-w-sm w-full mx-4">
            <div className="flex justify-between items-center mb-4">
              <h3 className="text-lg font-semibold text-gray-900 dark:text-white">
                My Profile QR Code
              </h3>
              <button
                onClick={closeQR}
                className="text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
              >
                ✕
              </button>
            </div>
            
            {qrDataURL ? (
              <div className="text-center">
                <img 
                  src={qrDataURL} 
                  alt="Profile QR Code" 
                  className="mx-auto mb-4 border rounded"
                />
                <p className="text-sm text-gray-600 dark:text-gray-400">
                  Share this QR code to let others add you as a friend
                </p>
              </div>
            ) : (
              <div className="text-center py-8">
                <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-600 mx-auto"></div>
                <p className="text-sm text-gray-500 mt-2">Generating QR code...</p>
              </div>
            )}
          </div>
        </div>
      )}

      {/* QR Scanner Modal */}
      {showScanner && (
        <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
          <div className="bg-white dark:bg-gray-800 p-6 rounded-lg max-w-md w-full mx-4">
            <div className="flex justify-between items-center mb-4">
              <h3 className="text-lg font-semibold text-gray-900 dark:text-white">
                Scan QR Code
              </h3>
              <button
                onClick={closeScanner}
                className="text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
              >
                ✕
              </button>
            </div>
            
            <div className="space-y-4">
              <video
                ref={videoRef}
                className="w-full rounded border aspect-square"
              />
              
              {scanResult && (
                <div className={`p-3 rounded text-sm ${
                  scanResult.includes('Added') || scanResult.includes('success') 
                    ? 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200'
                    : 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200'
                }`}>
                  {scanResult}
                </div>
              )}
              
              <p className="text-xs text-gray-500 dark:text-gray-400">
                Point your camera at a SnartNet profile QR code or magnet link QR code
              </p>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

export default QRCodeManager
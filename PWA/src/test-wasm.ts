import { getCore } from '@/lib/core'

// Test WASM integration
async function testWasmIntegration() {
  try {
    console.log('[Test] Starting WASM core test...')
    
    // Initialize core
    const core = await getCore()
    console.log('[Test] Core initialized successfully')
    
    // Test profile creation
    const magnetUri = await core.createProfile('testuser', 'Test User', 'This is a test profile')
    console.log('[Test] Profile created:', magnetUri)
    
    // Test getting current profile
    const profile = await core.getCurrentProfile()
    console.log('[Test] Current profile:', profile)
    
    // Test key operations
    const publicKey = await core.getPublicKey()
    const fingerprint = await core.getFingerprint()
    console.log('[Test] Public key:', publicKey)
    console.log('[Test] Fingerprint:', fingerprint)
    
    // Test post creation
    const post = await core.createPost('Hello from the Rust WASM core! 🦀', ['test', 'wasm'])
    console.log('[Test] Post created:', post)
    
    // Test message creation (using fingerprint as recipient)
    const message = await core.createMessage(fingerprint, 'Test message to myself!')
    console.log('[Test] Message created:', message)
    
    console.log('[Test] All WASM tests passed! ✅')
    
  } catch (error) {
    console.error('[Test] WASM test failed:', error)
  }
}

// Run test on page load
document.addEventListener('DOMContentLoaded', () => {
  console.log('[Test] Page loaded, starting WASM test in 2 seconds...')
  setTimeout(testWasmIntegration, 2000)
})

export { testWasmIntegration }
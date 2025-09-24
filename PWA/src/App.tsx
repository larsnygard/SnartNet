import { BrowserRouter, Routes, Route } from 'react-router-dom'
import HomePage from '@/pages/HomePage'
import ProfilePage from '@/pages/ProfilePage'
import MessagesPage from '@/pages/MessagesPage'
import DiscoverPage from '@/pages/DiscoverPage'
import Layout from '@/components/Layout'
import { useInitializeCore } from '@/hooks/useInitializeCore'
import './App.css'

function App() {
  // Initialize the WASM core and load existing profile
  useInitializeCore()
  return (
    <BrowserRouter basename="/net">
      <Layout>
        <Routes>
          <Route path="/" element={<HomePage />} />
          <Route path="/profile/:id?" element={<ProfilePage />} />
          <Route path="/discover" element={<DiscoverPage />} />
          <Route path="/messages/:id?" element={<MessagesPage />} />
        </Routes>
      </Layout>
    </BrowserRouter>
  )
}

export default App
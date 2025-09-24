import { BrowserRouter, Routes, Route } from 'react-router-dom'
import HomePage from '@/pages/HomePage'
import ProfilePage from '@/pages/ProfilePage'
import MessagesPage from '@/pages/MessagesPage'
import Layout from '@/components/Layout'
import './App.css'

function App() {
  return (
    <BrowserRouter>
      <Layout>
        <Routes>
          <Route path="/" element={<HomePage />} />
          <Route path="/profile/:id?" element={<ProfilePage />} />
          <Route path="/messages/:id?" element={<MessagesPage />} />
        </Routes>
      </Layout>
    </BrowserRouter>
  )
}

export default App
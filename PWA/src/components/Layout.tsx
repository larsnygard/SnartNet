import Navigation from './Navigation'

// Version injected via define (optional) else fallback unknown
const APP_VERSION = (import.meta as any).env?.VITE_APP_VERSION || 'dev'

interface LayoutProps {
  children: React.ReactNode
}

const Layout: React.FC<LayoutProps> = ({ children }) => {
  return (
    <div className="min-h-screen flex flex-col bg-gray-50 dark:bg-gray-900">
      <Navigation />
      <main className="container mx-auto px-4 py-8 flex-1 w-full">
        {children}
      </main>
      <footer className="border-t border-gray-200 dark:border-gray-700 py-4 text-center text-xs text-gray-500 dark:text-gray-400">
        <span>SnartNet v{APP_VERSION}</span>
        <span className="mx-2">•</span>
        <span>Development build</span>
      </footer>
    </div>
  )
}

export default Layout
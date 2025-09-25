import { Link } from 'react-router-dom'

const Navigation: React.FC = () => {
  return (
    <nav className="bg-white dark:bg-gray-800 shadow-sm">
      <div className="container mx-auto px-4">
        <div className="flex justify-between items-center h-16">
          <Link to="/" className="text-xl font-bold text-gray-900 dark:text-white">
            SnartNet
          </Link>
          <div className="flex space-x-4">
            <Link 
              to="/" 
              className="text-gray-600 hover:text-gray-900 dark:text-gray-300 dark:hover:text-white"
            >
              Home
            </Link>
            <Link 
              to="/profile" 
              className="text-gray-600 hover:text-gray-900 dark:text-gray-300 dark:hover:text-white"
            >
              Profile
            </Link>
            <Link 
              to="/discover" 
              className="text-gray-600 hover:text-gray-900 dark:text-gray-300 dark:hover:text-white"
            >
              Discover
            </Link>
            <Link 
              to="/contacts" 
              className="text-gray-600 hover:text-gray-900 dark:text-gray-300 dark:hover:text-white"
            >
              Contacts
            </Link>
            <Link 
              to="/messages" 
              className="text-gray-600 hover:text-gray-900 dark:text-gray-300 dark:hover:text-white"
            >
              Messages
            </Link>
          </div>
        </div>
      </div>
    </nav>
  )
}

export default Navigation
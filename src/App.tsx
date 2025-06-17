import React, { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import './App.css';

interface PasswordEntry {
  id: number;
  title: string;
  username: string;
  url?: string;
  notes?: string;
}

function App() {
  const [query, setQuery] = useState('');
  const [entries, setEntries] = useState<PasswordEntry[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [loading, setLoading] = useState(false);
  const appWindow = getCurrentWebviewWindow();

  const searchInputRef = useRef<HTMLInputElement>(null);

  // Focus search input when window is shown
  useEffect(() => {
    const focusInput = () => {
      setTimeout(() => {
        searchInputRef.current?.focus();
        searchInputRef.current?.select();
      }, 100);
    };

    // Listen for window focus events
    const unlisten = appWindow.onFocusChanged(({ payload: focused }) => {
      if (focused) {
        focusInput();
      }
    });

    // Initial focus
    focusInput();

    return () => {
      unlisten.then(f => f());
    };
  }, []);

  // Search entries when query changes
  useEffect(() => {
    const searchEntries = async () => {
      setLoading(true);
      try {
        const results = await invoke<PasswordEntry[]>('search_entries', { query });
        setEntries(results);
        setSelectedIndex(0);
      } catch (error) {
        console.error('Search failed:', error);
        setEntries([]);
      } finally {
        setLoading(false);
      }
    };

    const debounceTimer = setTimeout(searchEntries, 200);
    return () => clearTimeout(debounceTimer);
  }, [query]);

  // Keyboard navigation
  const handleKeyDown = (e: React.KeyboardEvent) => {
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        setSelectedIndex(prev => Math.min(prev + 1, entries.length - 1));
        break;
      case 'ArrowUp':
        e.preventDefault();
        setSelectedIndex(prev => Math.max(prev - 1, 0));
        break;
      case 'Enter':
        e.preventDefault();
        if (entries[selectedIndex]) {
          copyPassword(entries[selectedIndex].id);
        }
        break;
      case 'Tab':
        e.preventDefault();
        if (entries[selectedIndex]) {
          copyUsername(entries[selectedIndex].username);
        }
        break;
      case 'Escape':
        e.preventDefault();
        hideWindow();
        break;
    }
  };

  const copyPassword = async (entryId: number) => {
    try {
      await invoke('copy_password', { entryId });
      showNotification('Password copied to clipboard');
      hideWindow();
    } catch (error) {
      console.error('Failed to copy password:', error);
      showNotification('Failed to copy password', 'error');
    }
  };

  const copyUsername = async (username: string) => {
    try {
      await invoke('copy_username', { username });
      showNotification('Username copied to clipboard');
      hideWindow();
    } catch (error) {
      console.error('Failed to copy username:', error);
      showNotification('Failed to copy username', 'error');
    }
  };

  const hideWindow = async () => {
    try {
      await appWindow.hide();
      setQuery('');
      setSelectedIndex(0);
    } catch (error) {
      console.error('Failed to hide window:', error);
    }
  };

  const showNotification = (message: string, type: 'success' | 'error' = 'success') => {
    // Simple notification - could be improved with toast library
    console.log(`${type.toUpperCase()}: ${message}`);
  };

  const highlightMatch = (text: string, query: string) => {
    if (!query) return text;
    
    const index = text.toLowerCase().indexOf(query.toLowerCase());
    if (index === -1) return text;
    
    return (
      <>
        {text.slice(0, index)}
        <span className="highlight">{text.slice(index, index + query.length)}</span>
        {text.slice(index + query.length)}
      </>
    );
  };

  const getInitials = (title: string) => {
    return title
      .split(' ')
      .map(word => word[0])
      .join('')
      .toUpperCase()
      .slice(0, 2);
  };

  return (
    <div className="app">
      <div className="search-container">
        <div className="search-box">
          <svg 
            className="search-icon" 
            width="20" 
            height="20" 
            viewBox="0 0 24 24" 
            fill="none" 
            stroke="currentColor" 
            strokeWidth="2"
          >
            <circle cx="11" cy="11" r="8"/>
            <path d="m21 21-4.35-4.35"/>
          </svg>
          <input
            ref={searchInputRef}
            type="text"
            placeholder="Search passwords..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            className="search-input"
          />
        </div>
        
        <div className="shortcuts">
          <span className="shortcut">↵ Copy Password</span>
          <span className="shortcut">Tab Copy Username</span>
          <span className="shortcut">Esc Close</span>
        </div>
      </div>

      <div className="results-container">
        {loading ? (
          <div className="loading">Searching...</div>
        ) : entries.length === 0 ? (
          <div className="no-results">
            {query ? 'No passwords found' : 'Start typing to search...'}
          </div>
        ) : (
          <div className="results-list">
            {entries.map((entry, index) => (
              <div
                key={entry.id}
                className={`result-item ${index === selectedIndex ? 'selected' : ''}`}
                onClick={() => copyPassword(entry.id)}
                onMouseEnter={() => setSelectedIndex(index)}
              >
                <div className="entry-icon">
                  {getInitials(entry.title)}
                </div>
                <div className="entry-content">
                  <div className="entry-title">
                    {highlightMatch(entry.title, query)}
                  </div>
                  <div className="entry-username">
                    {highlightMatch(entry.username, query)}
                  </div>
                  {entry.url && (
                    <div className="entry-url">{entry.url}</div>
                  )}
                </div>
                <div className="entry-actions">
                  <button 
                    className="action-btn"
                    onClick={(e) => {
                      e.stopPropagation();
                      copyUsername(entry.username);
                    }}
                    title="Copy Username (Tab)"
                  >
                    👤
                  </button>
                  <button 
                    className="action-btn"
                    onClick={(e) => {
                      e.stopPropagation();
                      copyPassword(entry.id);
                    }}
                    title="Copy Password (Enter)"
                  >
                    🔑
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

export default App;
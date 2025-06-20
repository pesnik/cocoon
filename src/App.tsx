import React, { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import './App.css';

interface PasswordEntry {
  id: number;
  title: string;
  username: string;
  password: string;
  url?: string;
  notes?: string;
  created_at: string;
}

type View = 'search' | 'add' | 'edit';

function App() {
  const [view, setView] = useState<View>('search');
  const [query, setQuery] = useState('');
  const [entries, setEntries] = useState<PasswordEntry[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [loading, setLoading] = useState(false);
  const [editingEntry, setEditingEntry] = useState<PasswordEntry | null>(null);
  
  // Form state
  const [formData, setFormData] = useState({
    title: '',
    username: '',
    password: '',
    url: '',
    notes: ''
  });

  const appWindow = getCurrentWebviewWindow();
  const searchInputRef = useRef<HTMLInputElement>(null);
  const titleInputRef = useRef<HTMLInputElement>(null);

  // Focus input when window is shown
  useEffect(() => {
    const focusInput = () => {
      setTimeout(() => {
        if (view === 'search') {
          searchInputRef.current?.focus();
          searchInputRef.current?.select();
        } else if (view === 'add' || view === 'edit') {
          titleInputRef.current?.focus();
        }
      }, 100);
    };

    const unlisten = appWindow.onFocusChanged(({ payload: focused }) => {
      if (focused) {
        focusInput();
      }
    });

    focusInput();

    return () => {
      unlisten.then(f => f());
    };
  }, [view]);

  // Search entries when query changes
  useEffect(() => {
    if (view !== 'search') return;

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
  }, [query, view]);

  // Keyboard navigation for search view
  const handleSearchKeyDown = (e: React.KeyboardEvent) => {
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
      case 'Insert':
      case 'F2':
        e.preventDefault();
        openAddForm();
        break;
    }
  };

  // Form keyboard navigation
  const handleFormKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      setView('search');
      resetForm();
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
      setView('search');
      resetForm();
    } catch (error) {
      console.error('Failed to hide window:', error);
    }
  };

  const showNotification = (message: string, type: 'success' | 'error' = 'success') => {
    console.log(`${type.toUpperCase()}: ${message}`);
  };

  const openAddForm = () => {
    setView('add');
    resetForm();
    setEditingEntry(null);
  };

  const openEditForm = (entry: PasswordEntry) => {
    setView('edit');
    setEditingEntry(entry);
    setFormData({
      title: entry.title,
      username: entry.username,
      password: entry.password,
      url: entry.url || '',
      notes: entry.notes || ''
    });
  };

  const resetForm = () => {
    setFormData({
      title: '',
      username: '',
      password: '',
      url: '',
      notes: ''
    });
  };

  const handleFormSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    
    if (!formData.title.trim() || !formData.username.trim() || !formData.password.trim()) {
      showNotification('Title, username, and password are required', 'error');
      return;
    }

    try {
      if (view === 'add') {
        await invoke('add_entry', {
          title: formData.title.trim(),
          username: formData.username.trim(),
          password: formData.password,
          url: formData.url.trim() || null,
          notes: formData.notes.trim() || null
        });
        showNotification('Password entry added successfully');
      } else if (view === 'edit' && editingEntry) {
        await invoke('update_entry', {
          id: editingEntry.id,
          title: formData.title.trim(),
          username: formData.username.trim(),
          password: formData.password,
          url: formData.url.trim() || null,
          notes: formData.notes.trim() || null
        });
        showNotification('Password entry updated successfully');
      }
      
      setView('search');
      resetForm();
      setQuery(''); // Trigger refresh
    } catch (error) {
      console.error('Failed to save entry:', error);
      showNotification('Failed to save entry', 'error');
    }
  };

  const handleDelete = async (id: number) => {
    if (window.confirm('Are you sure you want to delete this entry?')) {
      try {
        await invoke('delete_entry', { id });
        showNotification('Password entry deleted');
        setQuery(''); // Trigger refresh
      } catch (error) {
        console.error('Failed to delete entry:', error);
        showNotification('Failed to delete entry', 'error');
      }
    }
  };

  const generatePassword = () => {
    const length = 16;
    const charset = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*';
    let password = '';
    for (let i = 0; i < length; i++) {
      password += charset.charAt(Math.floor(Math.random() * charset.length));
    }
    setFormData(prev => ({ ...prev, password }));
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

  if (view === 'add' || view === 'edit') {
    return (
      <div className="app">
        <div className="form-container">
          <div className="form-header">
            <h2>{view === 'add' ? 'Add New Entry' : 'Edit Entry'}</h2>
            <button 
              className="close-btn"
              onClick={() => { setView('search'); resetForm(); }}
              title="Close (Esc)"
            >
              ✕
            </button>
          </div>
          
          <form onSubmit={handleFormSubmit} onKeyDown={handleFormKeyDown}>
            <div className="form-group">
              <label htmlFor="title">Title *</label>
              <input
                ref={titleInputRef}
                id="title"
                type="text"
                value={formData.title}
                onChange={(e) => setFormData(prev => ({ ...prev, title: e.target.value }))}
                placeholder="e.g., GitHub, Gmail, AWS Console"
                required
              />
            </div>

            <div className="form-group">
              <label htmlFor="username">Username/Email *</label>
              <input
                id="username"
                type="text"
                value={formData.username}
                onChange={(e) => setFormData(prev => ({ ...prev, username: e.target.value }))}
                placeholder="john.doe@example.com"
                required
              />
            </div>

            <div className="form-group">
              <label htmlFor="password">Password *</label>
              <div className="password-input-group">
                <input
                  id="password"
                  type="password"
                  value={formData.password}
                  onChange={(e) => setFormData(prev => ({ ...prev, password: e.target.value }))}
                  placeholder="Enter password"
                  required
                />
                <button
                  type="button"
                  className="generate-btn"
                  onClick={generatePassword}
                  title="Generate Password"
                >
                  🎲
                </button>
              </div>
            </div>

            <div className="form-group">
              <label htmlFor="url">URL</label>
              <input
                id="url"
                type="url"
                value={formData.url}
                onChange={(e) => setFormData(prev => ({ ...prev, url: e.target.value }))}
                placeholder="https://example.com"
              />
            </div>

            <div className="form-group">
              <label htmlFor="notes">Notes</label>
              <textarea
                id="notes"
                value={formData.notes}
                onChange={(e) => setFormData(prev => ({ ...prev, notes: e.target.value }))}
                placeholder="Additional notes..."
                rows={3}
              />
            </div>

            <div className="form-actions">
              <button
                type="button"
                className="cancel-btn"
                onClick={() => { setView('search'); resetForm(); }}
              >
                Cancel
              </button>
              <button type="submit" className="save-btn">
                {view === 'add' ? 'Add Entry' : 'Update Entry'}
              </button>
            </div>
          </form>
        </div>
      </div>
    );
  }

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
            onKeyDown={handleSearchKeyDown}
            className="search-input"
          />
          <button
            className="add-btn"
            onClick={openAddForm}
            title="Add New Entry (Insert/F2)"
          >
            +
          </button>
        </div>
        
        <div className="shortcuts">
          <span className="shortcut">↵ Copy Password</span>
          <span className="shortcut">Tab Copy Username</span>
          <span className="shortcut">Insert/F2 Add</span>
          <span className="shortcut">Esc Close</span>
        </div>
      </div>

      <div className="results-container">
        {loading ? (
          <div className="loading">Searching...</div>
        ) : entries.length === 0 ? (
          <div className="no-results">
            {query ? 'No passwords found' : 'Start typing to search or press Insert to add a new entry...'}
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
                  <button 
                    className="action-btn"
                    onClick={(e) => {
                      e.stopPropagation();
                      openEditForm(entry);
                    }}
                    title="Edit Entry"
                  >
                    ✏️
                  </button>
                  <button 
                    className="action-btn delete"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleDelete(entry.id);
                    }}
                    title="Delete Entry"
                  >
                    🗑️
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
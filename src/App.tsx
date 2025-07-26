import React, { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import "./App.css";

interface PasswordEntry {
  id: number;
  title: string;
  username: string;
  password: string;
  url?: string;
  notes?: string;
  created_at: string;
}

type View = "search" | "add" | "edit";

function App() {
  const [view, setView] = useState<View>("search");
  const [query, setQuery] = useState("");
  const [entries, setEntries] = useState<PasswordEntry[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [loading, setLoading] = useState(false);
  const [editingEntry, setEditingEntry] = useState<PasswordEntry | null>(null);
  const [showPassword, setShowPassword] = useState(false);
  const [hasMasterPassword, setHasMasterPassword] = useState(false);
  const [masterPassword, setMasterPassword] = useState("");
  const [newMasterPassword, setNewMasterPassword] = useState("");
  const [confirmMasterPassword, setConfirmMasterPassword] = useState("");
  const [authError, setAuthError] = useState("");
  const [isAuthenticated, setIsAuthenticated] = useState(false);
  const [editPasswordAuth, setEditPasswordAuth] = useState("");
  const [editAuthError, setEditAuthError] = useState("");

  // Form state
  const [formData, setFormData] = useState({
    title: "",
    username: "",
    password: "",
    url: "",
    notes: "",
  });

  const appWindow = getCurrentWebviewWindow();
  const searchInputRef = useRef<HTMLInputElement>(null);
  const titleInputRef = useRef<HTMLInputElement>(null);
  const masterPasswordRef = useRef<HTMLInputElement>(null);
  const editPasswordAuthRef = useRef<HTMLInputElement>(null);

  // Enhanced focus management for Spotlight-like behavior
  useEffect(() => {
    const unlisten = listen('focus-search-input', () => {
      // Immediate focus without delay for snappy Spotlight-like experience
      if (searchInputRef.current && isAuthenticated && view === "search") {
        searchInputRef.current.focus();
        searchInputRef.current.select();
      }
    });

    return () => {
      unlisten.then(fn => fn());
    };
  }, [isAuthenticated, view]);

  // Enhanced focus management with better timing
  useEffect(() => {
    const focusInput = () => {
      // Reduced timeout for snappier response
      setTimeout(() => {
        if (view === "search" && isAuthenticated) {
          searchInputRef.current?.focus();
          searchInputRef.current?.select();
        } else if ((view === "add" || view === "edit") && isAuthenticated) {
          titleInputRef.current?.focus();
        } else if (hasMasterPassword && !isAuthenticated) {
          masterPasswordRef.current?.focus();
        } else if (!hasMasterPassword) {
          document.getElementById("newMasterPassword")?.focus();
        }
      }, 50); // Reduced from 100ms to 50ms
    };

    const unlisten = appWindow.onFocusChanged(({ payload: focused }) => {
      if (focused) {
        focusInput();
      }
    });

    focusInput();

    return () => {
      unlisten.then((f) => f());
    };
  }, [view, isAuthenticated, hasMasterPassword]);

  // Rest of your existing useEffect hooks...
  useEffect(() => {
    checkMasterPassword();
  }, []);

  useEffect(() => {
    if (view !== "search" || !isAuthenticated) return;

    const searchEntries = async () => {
      setLoading(true);
      try {
        const results = await invoke<PasswordEntry[]>("search_entries", {
          query,
          masterPassword,
        });
        setEntries(results);
        setSelectedIndex(0);
      } catch (error) {
        console.error("Search failed:", error);
        setEntries([]);
        if (error === "Invalid master password") {
          setIsAuthenticated(false);
          setAuthError("Invalid master password. Please re-enter.");
        }
      } finally {
        setLoading(false);
      }
    };

    const debounceTimer = setTimeout(searchEntries, 150); // Slightly faster debounce
    return () => clearTimeout(debounceTimer);
  }, [query, view, isAuthenticated]);

  // Enhanced keyboard navigation with better Spotlight-like shortcuts
  const handleSearchKeyDown = (e: React.KeyboardEvent) => {
    if (!isAuthenticated) return;

    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        setSelectedIndex((prev) => Math.min(prev + 1, entries.length - 1));
        break;
      case "ArrowUp":
        e.preventDefault();
        setSelectedIndex((prev) => Math.max(prev - 1, 0));
        break;
      case "Enter":
        e.preventDefault();
        if (entries[selectedIndex]) {
          // Auto-fill with enhanced focus management
          autoFillCredentials(entries[selectedIndex].id);
        }
        break;
      case "Tab":
        e.preventDefault();
        if (entries[selectedIndex]) {
          // Type username with enhanced focus management
          typeUsername(entries[selectedIndex].id);
        }
        break;
      case " ": // Spacebar
        e.preventDefault();
        if (entries[selectedIndex]) {
          // Type password with enhanced focus management
          typePassword(entries[selectedIndex].id);
        }
        break;
      case "Escape":
        e.preventDefault();
        hideWindow();
        break;
      case "Insert":
      case "F2":
        e.preventDefault();
        openAddForm();
        break;
      // Enhanced shortcuts for better UX
      case "Delete":
      case "Backspace":
        if (e.metaKey || e.ctrlKey) { // Cmd+Delete or Ctrl+Delete
          e.preventDefault();
          if (entries[selectedIndex]) {
            handleDelete(entries[selectedIndex].id);
          }
        }
        break;
      case "e":
        if (e.metaKey || e.ctrlKey) { // Cmd+E or Ctrl+E to edit
          e.preventDefault();
          if (entries[selectedIndex]) {
            openEditForm(entries[selectedIndex]);
          }
        }
        break;
    }
  };

  // Enhanced typing functions using the new Spotlight-aware commands
  const typeUsername = async (entryId: number) => {
    if (!isAuthenticated) return;
    try {
      await invoke("type_username_spotlight", { entryId, masterPassword });
      showNotification("Username typed to active field");
    } catch (error) {
      console.error("Failed to type username:", error);
      showNotification("Failed to type username", "error");
      if (error === "Invalid master password") {
        setIsAuthenticated(false);
        setAuthError("Invalid master password. Please re-enter.");
      }
    }
  };

  const typePassword = async (entryId: number) => {
    if (!isAuthenticated) return;
    try {
      await invoke("type_password_spotlight", { entryId, masterPassword });
      showNotification("Password typed to active field");
    } catch (error) {
      console.error("Failed to type password:", error);
      showNotification("Failed to type password", "error");
      if (error === "Invalid master password") {
        setIsAuthenticated(false);
        setAuthError("Invalid master password. Please re-enter.");
      }
    }
  };

  const autoFillCredentials = async (entryId: number) => {
    if (!isAuthenticated) return;
    try {
      await invoke("auto_fill_credentials_spotlight_with_login", { entryId, masterPassword, pressEnter: true });
      showNotification("Credentials auto-filled to login form");
    } catch (error) {
      console.error("Failed to auto-fill credentials:", error);
      showNotification("Failed to auto-fill credentials", "error");
      if (error === "Invalid master password") {
        setIsAuthenticated(false);
        setAuthError("Invalid master password. Please re-enter.");
      }
    }
  };

  // Enhanced window hiding with state reset
  const hideWindow = async () => {
    try {
      await appWindow.hide();
      // Reset state for next usage
      setQuery("");
      setSelectedIndex(0);
      setView("search");
      resetForm();
    } catch (error) {
      console.error("Failed to hide window:", error);
    }
  };

  // Rest of your existing functions remain the same...
  const checkMasterPassword = async () => {
    try {
      const exists = await invoke<boolean>("has_master_password");
      setHasMasterPassword(exists);
      if (!exists) {
        setIsAuthenticated(false);
      }
    } catch (error) {
      console.error("Failed to check master password:", error);
    }
  };

  const setupMasterPassword = async (e: React.FormEvent) => {
    e.preventDefault();

    if (newMasterPassword !== confirmMasterPassword) {
      setAuthError("Passwords do not match");
      return;
    }

    if (newMasterPassword.length < 8) {
      setAuthError("Password must be at least 8 characters long");
      return;
    }

    try {
      await invoke("setup_master_password", { password: newMasterPassword });
      setHasMasterPassword(true);
      setMasterPassword(newMasterPassword);
      setIsAuthenticated(true);
      setNewMasterPassword("");
      setConfirmMasterPassword("");
      setAuthError("");
    } catch (error) {
      setAuthError(error as string);
    }
  };

  const authenticate = async (e?: React.FormEvent) => {
    if (e) {
      e.preventDefault();
    }

    if (!masterPassword.trim()) {
      setAuthError("Please enter your master password");
      return;
    }

    try {
      await invoke("verify_master_password", {
        password: masterPassword,
      });
      setIsAuthenticated(true);
      setAuthError("");
    } catch (error) {
      console.log(error);
      setAuthError("Invalid master password");
      setMasterPassword("");
    }
  };

  const showNotification = (
    message: string,
    type: "success" | "error" = "success"
  ) => {
    console.log(`${type.toUpperCase()}: ${message}`);
  };

  const openAddForm = () => {
    if (!isAuthenticated) return;
    setView("add");
    resetForm();
    setEditingEntry(null);
  };

  const openEditForm = (entry: PasswordEntry) => {
    if (!isAuthenticated) return;
    setView("edit");
    setEditingEntry(entry);
    setFormData({
      title: entry.title,
      username: entry.username,
      password: "",
      url: entry.url || "",
      notes: entry.notes || "",
    });
    setShowPassword(false);
    setEditPasswordAuth("");
    setEditAuthError("");
    setTimeout(() => {
      editPasswordAuthRef.current?.focus();
    }, 100);
  };

  const resetForm = () => {
    setFormData({
      title: "",
      username: "",
      password: "",
      url: "",
      notes: "",
    });
  };

  const handleFormSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (
      !formData.title.trim() ||
      !formData.username.trim() ||
      !formData.password.trim()
    ) {
      showNotification("Title, username, and password are required", "error");
      return;
    }

    try {
      if (view === "add") {
        await invoke("add_entry", {
          title: formData.title.trim(),
          username: formData.username.trim(),
          password: formData.password,
          url: formData.url.trim() || null,
          notes: formData.notes.trim() || null,
          masterPassword,
        });
        showNotification("Password entry added successfully");
      } else if (view === "edit" && editingEntry) {
        await invoke("update_entry", {
          id: editingEntry.id,
          title: formData.title.trim(),
          username: formData.username.trim(),
          password: formData.password,
          url: formData.url.trim() || null,
          notes: formData.notes.trim() || null,
          masterPassword,
        });
        showNotification("Password entry updated successfully");
      }

      setView("search");
      resetForm();
      setQuery("");
    } catch (error) {
      console.error("Failed to save entry:", error);
      showNotification("Failed to save entry", "error");
      if (error === "Invalid master password") {
        setIsAuthenticated(false);
        setAuthError("Invalid master password. Please re-enter.");
      }
    }
  };

  const handleDelete = async (id: number) => {
    if (!isAuthenticated) return;
    try {
      await invoke("delete_entry", { id, masterPassword });
      showNotification("Password entry deleted");

      const results = await invoke<PasswordEntry[]>("search_entries", {
        query: "",
        masterPassword,
      });
      setEntries(results);
      setSelectedIndex(0);
    } catch (error) {
      console.error("Failed to delete entry:", error);
      showNotification("Failed to delete entry", "error");
      if (error === "Invalid master password") {
        setIsAuthenticated(false);
        setAuthError("Invalid master password. Please re-enter.");
      }
    }
  };

  const generatePassword = async () => {
    try {
      const password = await invoke<string>("generate_password", {
        length: 16,
        includeUppercase: true,
        includeLowercase: true,
        includeNumbers: true,
        includeSymbols: true,
      });
      setFormData((prev) => ({ ...prev, password }));
    } catch (error) {
      console.error("Failed to generate password:", error);
      const length = 16;
      const charset =
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*";
      let password = "";
      for (let i = 0; i < length; i++) {
        password += charset.charAt(Math.floor(Math.random() * charset.length));
      }
      setFormData((prev) => ({ ...prev, password }));
    }
  };

  const highlightMatch = (text: string, query: string) => {
    if (!query) return text;

    const index = text.toLowerCase().indexOf(query.toLowerCase());
    if (index === -1) return text;

    return (
      <>
        {text.slice(0, index)}
        <span className="highlight">
          {text.slice(index, index + query.length)}
        </span>
        {text.slice(index + query.length)}
      </>
    );
  };

  const getInitials = (title: string) => {
    return title
      .split(" ")
      .map((word) => word[0])
      .join("")
      .toUpperCase()
      .slice(0, 2);
  };

  const handleEditPasswordAuth = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!editPasswordAuth.trim()) {
      setEditAuthError("Please enter your master password to view.");
      return;
    }

    try {
      await invoke("verify_master_password", {
        password: editPasswordAuth,
      });
      if (editingEntry) {
        const entryWithPassword = await invoke<PasswordEntry>(
          "get_entry_by_id",
          {
            id: editingEntry.id,
            masterPassword: editPasswordAuth,
          }
        );
        setFormData((prev) => ({
          ...prev,
          password: entryWithPassword.password,
        }));
      }
      setEditAuthError("");
      setShowPassword(true);
    } catch (error) {
      console.log(error);
      setEditAuthError("Invalid master password");
      setEditPasswordAuth("");
      setShowPassword(false);
    }
  };

  // Form keyboard navigation
  const handleFormKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      setView("search");
      resetForm();
    }
  };

  // Render logic based on authentication and view state
  if (!hasMasterPassword) {
    return (
      <div className="app spotlight-style">
        <div className="auth-container">
          <div className="auth-header">
            <h2>🔒 Setup Master Password</h2>
            <p>Secure your password vault with a master password</p>
          </div>
          <form onSubmit={setupMasterPassword} className="auth-form">
            <div className="form-group">
              <input
                id="newMasterPassword"
                type="password"
                value={newMasterPassword}
                onChange={(e) => setNewMasterPassword(e.target.value)}
                placeholder="Master password (min 8 chars)"
                className="spotlight-input"
                required
                autoFocus
              />
            </div>
            <div className="form-group">
              <input
                id="confirmMasterPassword"
                type="password"
                value={confirmMasterPassword}
                onChange={(e) => setConfirmMasterPassword(e.target.value)}
                placeholder="Confirm master password"
                className="spotlight-input"
                required
              />
            </div>
            {authError && <div className="error-message">{authError}</div>}
            <button type="submit" className="spotlight-button primary">
              Create Vault
            </button>
          </form>
        </div>
      </div>
    );
  }

  if (!isAuthenticated) {
    return (
      <div className="app spotlight-style">
        <div className="auth-container">
          <div className="auth-header">
            <h2>🔓 Enter Master Password</h2>
            <p>Unlock your password vault</p>
          </div>
          <form onSubmit={authenticate} className="auth-form">
            <div className="form-group">
              <input
                ref={masterPasswordRef}
                id="masterPassword"
                type="password"
                value={masterPassword}
                onChange={(e) => {
                  setMasterPassword(e.target.value);
                  setAuthError("");
                }}
                placeholder="Enter master password"
                className="spotlight-input"
                required
                autoFocus
              />
            </div>
            {authError && <div className="error-message">{authError}</div>}
            <button type="submit" className="spotlight-button primary">
              Unlock Vault
            </button>
          </form>
        </div>
      </div>
    );
  }

  if (view === "add" || view === "edit") {
    return (
      <div className="app spotlight-style">
        <div className="form-container">
          <div className="form-header">
            <h2>{view === "add" ? "➕ Add New Entry" : "✏️ Edit Entry"}</h2>
            <button
              className="close-btn"
              onClick={() => {
                setView("search");
                resetForm();
              }}
              title="Close (Esc)"
            >
              ✕
            </button>
          </div>

          <form onSubmit={handleFormSubmit} onKeyDown={handleFormKeyDown} className="entry-form">
            <div className="form-group">
              <input
                ref={titleInputRef}
                id="title"
                type="text"
                value={formData.title}
                onChange={(e) =>
                  setFormData((prev) => ({ ...prev, title: e.target.value }))
                }
                placeholder="Title (e.g., GitHub, Gmail, AWS Console)"
                className="spotlight-input"
                required
                autoFocus
              />
            </div>

            <div className="form-group">
              <input
                id="username"
                type="text"
                value={formData.username}
                onChange={(e) =>
                  setFormData((prev) => ({ ...prev, username: e.target.value }))
                }
                placeholder="Username or Email"
                className="spotlight-input"
                required
              />
            </div>

            <div className="form-group">
              <div className="password-input-group">
                {view === "edit" && !showPassword ? (
                  <div className="password-auth-overlay">
                    <input
                      ref={editPasswordAuthRef}
                      type="password"
                      value={editPasswordAuth}
                      onChange={(e) => {
                        setEditPasswordAuth(e.target.value);
                        setEditAuthError("");
                      }}
                      placeholder="Enter master password to view/edit"
                      className="spotlight-input"
                      autoFocus
                    />
                    <button
                      type="button"
                      onClick={handleEditPasswordAuth}
                      className="spotlight-button secondary small"
                    >
                      Unlock
                    </button>
                    {editAuthError && (
                      <div className="error-message small">{editAuthError}</div>
                    )}
                  </div>
                ) : (
                  <div className="password-field">
                    <input
                      id="password"
                      type={showPassword ? "text" : "password"}
                      value={formData.password}
                      onChange={(e) =>
                        setFormData((prev) => ({
                          ...prev,
                          password: e.target.value,
                        }))
                      }
                      placeholder="Password"
                      className="spotlight-input"
                      required
                    />
                    <div className="password-actions">
                      <button
                        type="button"
                        className="icon-btn"
                        onClick={() => setShowPassword(!showPassword)}
                        title={showPassword ? "Hide Password" : "Show Password"}
                      >
                        {showPassword ? "🙈" : "👁️"}
                      </button>
                      <button
                        type="button"
                        className="icon-btn"
                        onClick={generatePassword}
                        title="Generate Password"
                      >
                        🎲
                      </button>
                    </div>
                  </div>
                )}
              </div>
            </div>

            <div className="form-group">
              <input
                id="url"
                type="url"
                value={formData.url}
                onChange={(e) =>
                  setFormData((prev) => ({ ...prev, url: e.target.value }))
                }
                placeholder="URL (optional)"
                className="spotlight-input"
              />
            </div>

            <div className="form-group">
              <textarea
                id="notes"
                value={formData.notes}
                onChange={(e) =>
                  setFormData((prev) => ({ ...prev, notes: e.target.value }))
                }
                placeholder="Notes (optional)"
                className="spotlight-textarea"
                rows={3}
              />
            </div>

            <div className="form-actions">
              <button
                type="button"
                className="spotlight-button secondary"
                onClick={() => {
                  setView("search");
                  resetForm();
                }}
              >
                Cancel
              </button>
              <button type="submit" className="spotlight-button primary">
                {view === "add" ? "Add Entry" : "Update Entry"}
              </button>
            </div>
          </form>
        </div>
      </div>
    );
  }

  return (
    <div className="app spotlight-style">
      <div className="search-container">
        <div className="search-box">
          <div className="search-icon">🔍</div>
          <input
            ref={searchInputRef}
            type="text"
            placeholder="Search passwords..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleSearchKeyDown}
            className="search-input spotlight-input"
            disabled={!isAuthenticated}
            autoFocus
          />
          <button
            className="add-btn icon-btn"
            onClick={openAddForm}
            title="Add New Entry (Insert/F2)"
            disabled={!isAuthenticated}
          >
            ➕
          </button>
        </div>

        <div className="shortcuts-bar">
          <div className="shortcuts">
            <span className="shortcut">
              <kbd>↵</kbd> Auto-Fill
            </span>
            <span className="shortcut">
              <kbd>Tab</kbd> Username
            </span>
            <span className="shortcut">
              <kbd>Space</kbd> Password
            </span>
            <span className="shortcut">
              <kbd>⌘E</kbd> Edit
            </span>
            <span className="shortcut">
              <kbd>Esc</kbd> Close
            </span>
          </div>
        </div>
      </div>

      <div className="results-container">
        {loading && isAuthenticated ? (
          <div className="loading-state">
            <div className="loading-spinner"></div>
            <span>Searching...</span>
          </div>
        ) : entries.length === 0 ? (
          <div className="no-results">
            {isAuthenticated ? (
              query ? (
                <div className="empty-state">
                  <div className="empty-icon">🔍</div>
                  <h3>No passwords found</h3>
                  <p>Try a different search term or add a new entry</p>
                </div>
              ) : (
                <div className="empty-state">
                  <div className="empty-icon">🔐</div>
                  <h3>Your vault is ready</h3>
                  <p>Start typing to search or press <kbd>Insert</kbd> to add your first password</p>
                </div>
              )
            ) : (
              <div className="empty-state">
                <div className="empty-icon">🔒</div>
                <h3>Please authenticate</h3>
                <p>Enter your master password to access your vault</p>
              </div>
            )}
          </div>
        ) : (
          <div className="results-list">
            {entries.map((entry, index) => (
              <div
                key={entry.id}
                className={`result-item ${
                  index === selectedIndex ? "selected" : ""
                }`}
                onClick={() => autoFillCredentials(entry.id)}
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
                    <div className="entry-url">
                      {highlightMatch(entry.url, query)}
                    </div>
                  )}
                </div>
                <div className="entry-actions">
                  <button
                    className="action-btn"
                    onClick={(e) => {
                      e.stopPropagation();
                      typeUsername(entry.id);
                    }}
                    title="Type Username (Tab)"
                  >
                    👤
                  </button>
                  <button
                    className="action-btn"
                    onClick={(e) => {
                      e.stopPropagation();
                      typePassword(entry.id);
                    }}
                    title="Type Password (Space)"
                  >
                    🔑
                  </button>
                  <button
                    className="action-btn primary"
                    onClick={(e) => {
                      e.stopPropagation();
                      autoFillCredentials(entry.id);
                    }}
                    title="Auto-Fill Login (Enter)"
                  >
                    🚀
                  </button>
                  <button
                    className="action-btn"
                    onClick={(e) => {
                      e.stopPropagation();
                      openEditForm(entry);
                    }}
                    title="Edit Entry (⌘E)"
                  >
                    ✏️
                  </button>
                  <button
                    className="action-btn danger"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleDelete(entry.id);
                    }}
                    title="Delete Entry (⌘Delete)"
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
import React, { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
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

  // Focus management
  useEffect(() => {
    const focusInput = () => {
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
      }, 100);
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

  // Check master password existence on app start
  useEffect(() => {
    checkMasterPassword();
  }, []);

  // Search entries when query changes and authenticated
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

    const debounceTimer = setTimeout(searchEntries, 200);
    return () => clearTimeout(debounceTimer);
  }, [query, view, isAuthenticated]);

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

  // Keyboard navigation for search view
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
          // Auto-fill both username and password
          autoFillCredentials(entries[selectedIndex].id);
        }
        break;
      case "Tab":
        e.preventDefault();
        if (entries[selectedIndex]) {
          // Type only username
          typeUsername(entries[selectedIndex].id);
        }
        break;
      case " ": // Spacebar
        e.preventDefault();
        if (entries[selectedIndex]) {
          // Type only password
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

  // NEW DIRECT INPUT FUNCTIONS
  const typeUsername = async (entryId: number) => {
    if (!isAuthenticated) return;
    try {
      await invoke("type_username", { entryId, masterPassword });
      showNotification("Username typed directly to active field");
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
      await invoke("type_password", { entryId, masterPassword });
      showNotification("Password typed directly to active field");
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
      await invoke("auto_fill_credentials", { entryId, masterPassword });
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

  const hideWindow = async () => {
    try {
      await appWindow.hide();
      setQuery("");
      setSelectedIndex(0);
      setView("search");
      resetForm();
    } catch (error) {
      console.error("Failed to hide window:", error);
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

  // Render logic based on authentication and view state
  if (!hasMasterPassword) {
    return (
      <div className="app">
        <div className="auth-container">
          <h2>Setup Master Password</h2>
          <form onSubmit={setupMasterPassword}>
            <div className="form-group">
              <label htmlFor="newMasterPassword">Master Password</label>
              <input
                id="newMasterPassword"
                type="password"
                value={newMasterPassword}
                onChange={(e) => setNewMasterPassword(e.target.value)}
                placeholder="Enter master password (min 8 chars)"
                required
              />
            </div>
            <div className="form-group">
              <label htmlFor="confirmMasterPassword">Confirm Password</label>
              <input
                id="confirmMasterPassword"
                type="password"
                value={confirmMasterPassword}
                onChange={(e) => setConfirmMasterPassword(e.target.value)}
                placeholder="Confirm master password"
                required
              />
            </div>
            {authError && <div className="error">{authError}</div>}
            <button type="submit">Setup Master Password</button>
          </form>
        </div>
      </div>
    );
  }

  if (!isAuthenticated) {
    return (
      <div className="app">
        <div className="auth-container">
          <h2>Enter Master Password</h2>
          <form onSubmit={authenticate}>
            <div className="form-group">
              <label htmlFor="masterPassword">Master Password</label>
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
                required
              />
            </div>
            {authError && <div className="error">{authError}</div>}
            <button type="submit">Continue</button>
          </form>
        </div>
      </div>
    );
  }

  if (view === "add" || view === "edit") {
    return (
      <div className="app">
        <div className="form-container">
          <div className="form-header">
            <h2>{view === "add" ? "Add New Entry" : "Edit Entry"}</h2>
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

          <form onSubmit={handleFormSubmit} onKeyDown={handleFormKeyDown}>
            <div className="form-group">
              <label htmlFor="title">Title *</label>
              <input
                ref={titleInputRef}
                id="title"
                type="text"
                value={formData.title}
                onChange={(e) =>
                  setFormData((prev) => ({ ...prev, title: e.target.value }))
                }
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
                onChange={(e) =>
                  setFormData((prev) => ({ ...prev, username: e.target.value }))
                }
                placeholder="john.doe@example.com"
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
                    />
                    <button
                      type="button"
                      onClick={handleEditPasswordAuth}
                      className="auth-password-btn"
                    >
                      Unlock
                    </button>
                    {editAuthError && (
                      <div className="error-small">{editAuthError}</div>
                    )}
                  </div>
                ) : (
                  <>
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
                      placeholder="Enter password"
                      required
                    />
                    <button
                      type="button"
                      className="toggle-password-btn"
                      onClick={() => setShowPassword(!showPassword)}
                      title={showPassword ? "Hide Password" : "Show Password"}
                    >
                      {showPassword ? "👁️" : "🙈"}
                    </button>
                    <button
                      type="button"
                      className="generate-btn"
                      onClick={generatePassword}
                      title="Generate Password"
                    >
                      🎲
                    </button>
                  </>
                )}
              </div>
            </div>

            <div className="form-group">
              <label htmlFor="url">URL</label>
              <input
                id="url"
                type="url"
                value={formData.url}
                onChange={(e) =>
                  setFormData((prev) => ({ ...prev, url: e.target.value }))
                }
                placeholder="https://example.com"
              />
            </div>

            <div className="form-group">
              <label htmlFor="notes">Notes</label>
              <textarea
                id="notes"
                value={formData.notes}
                onChange={(e) =>
                  setFormData((prev) => ({ ...prev, notes: e.target.value }))
                }
                placeholder="Additional notes..."
                rows={3}
              />
            </div>

            <div className="form-actions">
              <button
                type="button"
                className="cancel-btn"
                onClick={() => {
                  setView("search");
                  resetForm();
                }}
              >
                Cancel
              </button>
              <button type="submit" className="save-btn">
                {view === "add" ? "Add Entry" : "Update Entry"}
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
            <circle cx="11" cy="11" r="8" />
            <path d="m21 21-4.35-4.35" />
          </svg>
          <input
            ref={searchInputRef}
            type="text"
            placeholder="Search passwords..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleSearchKeyDown}
            className="search-input"
            disabled={!isAuthenticated}
          />
          <button
            className="add-btn"
            onClick={openAddForm}
            title="Add New Entry (Insert/F2)"
            disabled={!isAuthenticated}
          >
            +
          </button>
        </div>

        <div className="shortcuts">
          <span className="shortcut">↵ Auto-Fill Login</span>
          <span className="shortcut">Tab Type Username</span>
          <span className="shortcut">Space Type Password</span>
          <span className="shortcut">Insert/F2 Add</span>
          <span className="shortcut">Esc Close</span>
        </div>
      </div>

      <div className="results-container">
        {loading && isAuthenticated ? (
          <div className="loading">Searching...</div>
        ) : entries.length === 0 ? (
          <div className="no-results">
            {isAuthenticated
              ? query
                ? "No passwords found"
                : "Start typing to search or press Insert to add a new entry..."
              : "Please enter your master password to get started."}
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
                <div className="entry-icon">{getInitials(entry.title)}</div>
                <div className="entry-content">
                  <div className="entry-title">
                    {highlightMatch(entry.title, query)}
                  </div>
                  <div className="entry-username">
                    {highlightMatch(entry.username, query)}
                  </div>
                  {entry.url && <div className="entry-url">{entry.url}</div>}
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
                    className="action-btn"
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
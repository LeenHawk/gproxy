//! File service: user credential files and Claude file metadata.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwap;

use crate::types::{MemoryClaudeFile, MemoryUserCredentialFile};

/// Manages user-uploaded file records and Claude file metadata cache.
pub struct FileService {
    user_files: ArcSwap<Vec<MemoryUserCredentialFile>>,
    claude_files: ArcSwap<HashMap<(i64, String), MemoryClaudeFile>>,
    /// Serializes single-item write operations to prevent lost updates.
    write_lock: Mutex<()>,
}

impl FileService {
    /// Creates a new empty file service.
    pub fn new() -> Self {
        Self {
            user_files: ArcSwap::from(Arc::new(Vec::new())),
            claude_files: ArcSwap::from(Arc::new(HashMap::new())),
            write_lock: Mutex::new(()),
        }
    }

    /// Find a specific active file by user, provider, and file ID.
    pub fn find_user_file(
        &self,
        user_id: i64,
        provider_id: i64,
        file_id: &str,
    ) -> Option<MemoryUserCredentialFile> {
        self.user_files
            .load()
            .iter()
            .find(|r| {
                r.active
                    && r.user_id == user_id
                    && r.provider_id == provider_id
                    && r.file_id == file_id
            })
            .cloned()
    }

    /// List all active files for a user on a provider.
    pub fn list_user_files(&self, user_id: i64, provider_id: i64) -> Vec<MemoryUserCredentialFile> {
        self.user_files
            .load()
            .iter()
            .filter(|r| r.active && r.user_id == user_id && r.provider_id == provider_id)
            .cloned()
            .collect()
    }

    /// Find Claude file metadata by provider ID and file ID.
    pub fn find_claude_file(&self, provider_id: i64, file_id: &str) -> Option<MemoryClaudeFile> {
        self.claude_files
            .load()
            .get(&(provider_id, file_id.to_string()))
            .cloned()
    }

    // -- Bulk replace (bootstrap / reload) --

    /// Replace all user files atomically.
    pub fn replace_user_files(&self, files: Vec<MemoryUserCredentialFile>) {
        self.user_files.store(Arc::new(files));
    }

    /// Replace all Claude file metadata atomically.
    pub fn replace_claude_files(&self, files: HashMap<(i64, String), MemoryClaudeFile>) {
        self.claude_files.store(Arc::new(files));
    }

    // -- Single-item CRUD --

    /// Upsert a user file record.
    pub fn upsert_user_file(&self, file: MemoryUserCredentialFile) {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut files = (*self.user_files.load_full()).clone();
        let key = (file.user_id, file.provider_id, file.file_id.clone());
        if let Some(existing) = files.iter_mut().find(|f| {
            (f.user_id, f.provider_id, f.file_id.as_str()) == (key.0, key.1, key.2.as_str())
        }) {
            *existing = file;
        } else {
            files.push(file);
        }
        self.user_files.store(Arc::new(files));
    }

    /// Remove all files for a user.
    pub fn remove_user_files_for_user(&self, user_id: i64) {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut files = (*self.user_files.load_full()).clone();
        files.retain(|f| f.user_id != user_id);
        self.user_files.store(Arc::new(files));
    }

    /// Upsert a Claude file metadata record.
    pub fn upsert_claude_file(&self, file: MemoryClaudeFile) {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut files = (*self.claude_files.load_full()).clone();
        files.insert((file.provider_id, file.file_id.clone()), file);
        self.claude_files.store(Arc::new(files));
    }
}

impl Default for FileService {
    fn default() -> Self {
        Self::new()
    }
}

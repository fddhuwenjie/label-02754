//! SQL file scanner module.
//!
//! Discovers and manages SQL files in a directory tree for execution.

use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use thiserror::Error;

/// Scanner operation errors.
#[derive(Error, Debug)]
pub enum ScannerError {
    /// Scan directory does not exist
    #[error("Directory not found: {0}")]
    DirectoryNotFound(String),
    /// File system I/O error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    /// Failed to read SQL file content
    #[error("Failed to read SQL file: {0}")]
    ReadError(String),
}

/// Represents a discovered SQL file with its metadata.
#[derive(Debug, Clone)]
pub struct SqlFile {
    /// Absolute path to the SQL file
    pub absolute_path: PathBuf,
    /// Path relative to the scan directory
    pub relative_path: String,
    /// Path where the JSON output will be written
    pub json_path: PathBuf,
}

impl SqlFile {
    /// Create a new SqlFile instance.
    ///
    /// # Arguments
    /// * `absolute_path` - Full path to the SQL file
    /// * `base_dir` - Base directory for calculating relative path
    pub fn new(absolute_path: PathBuf, base_dir: &Path) -> Self {
        let relative_path = absolute_path
            .strip_prefix(base_dir)
            .unwrap_or(&absolute_path)
            .to_string_lossy()
            .to_string();
        
        let json_path = absolute_path.with_extension("json");
        
        Self {
            absolute_path,
            relative_path,
            json_path,
        }
    }

    /// Read the SQL file content.
    pub fn read_content(&self) -> Result<String, ScannerError> {
        fs::read_to_string(&self.absolute_path)
            .map_err(|e| ScannerError::ReadError(format!("{}: {}", self.absolute_path.display(), e)))
    }

    /// Check if the corresponding JSON output file exists.
    pub fn json_exists(&self) -> bool {
        self.json_path.exists()
    }

    /// Get the last modified time of the JSON output file.
    pub fn json_modified_time(&self) -> Option<std::time::SystemTime> {
        fs::metadata(&self.json_path)
            .ok()
            .and_then(|m| m.modified().ok())
    }
}

/// SQL file directory scanner.
pub struct Scanner {
    base_directory: PathBuf,
}

impl Scanner {
    /// Create a new scanner for the specified directory.
    ///
    /// # Arguments
    /// * `base_directory` - Directory to scan for SQL files
    pub fn new<P: AsRef<Path>>(base_directory: P) -> Result<Self, ScannerError> {
        let base_directory = base_directory.as_ref().to_path_buf();
        if !base_directory.exists() {
            return Err(ScannerError::DirectoryNotFound(
                base_directory.display().to_string(),
            ));
        }
        Ok(Self { base_directory })
    }

    /// Scan the directory tree for SQL files.
    ///
    /// Recursively finds all `.sql` files in the base directory and subdirectories.
    pub fn scan(&self) -> Result<Vec<SqlFile>, ScannerError> {
        let mut sql_files = Vec::new();

        for entry in WalkDir::new(&self.base_directory)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext.eq_ignore_ascii_case("sql") {
                        sql_files.push(SqlFile::new(path.to_path_buf(), &self.base_directory));
                    }
                }
            }
        }

        Ok(sql_files)
    }

    #[allow(dead_code)]
    pub fn base_directory(&self) -> &Path {
        &self.base_directory
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_scanner_finds_sql_files() {
        let dir = tempdir().unwrap();
        let sql_dir = dir.path().join("sql");
        fs::create_dir_all(&sql_dir).unwrap();
        
        // Create test SQL files
        fs::write(sql_dir.join("test1.sql"), "SELECT 1").unwrap();
        fs::write(sql_dir.join("test2.SQL"), "SELECT 2").unwrap();
        fs::write(sql_dir.join("not_sql.txt"), "not sql").unwrap();
        
        // Create subdirectory with SQL file
        let sub_dir = sql_dir.join("subdir");
        fs::create_dir_all(&sub_dir).unwrap();
        fs::write(sub_dir.join("test3.sql"), "SELECT 3").unwrap();
        
        let scanner = Scanner::new(&sql_dir).unwrap();
        let files = scanner.scan().unwrap();
        
        assert_eq!(files.len(), 3);
    }

    #[test]
    fn test_sql_file_json_path() {
        let dir = tempdir().unwrap();
        let sql_path = dir.path().join("test.sql");
        fs::write(&sql_path, "SELECT 1").unwrap();
        
        let sql_file = SqlFile::new(sql_path.clone(), dir.path());
        
        assert_eq!(sql_file.json_path, dir.path().join("test.json"));
        assert!(!sql_file.json_exists());
    }
}

//! SQL 文件扫描模块
//!
//! 在目录树中发现和管理待执行的 SQL 文件。

use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use thiserror::Error;

/// 扫描器操作错误
#[derive(Error, Debug)]
pub enum ScannerError {
    /// 扫描目录不存在
    #[error("目录未找到: {0}")]
    DirectoryNotFound(String),
    /// 文件系统 I/O 错误
    #[error("IO 错误: {0}")]
    IoError(#[from] std::io::Error),
    /// 读取 SQL 文件内容失败
    #[error("读取 SQL 文件失败: {0}")]
    ReadError(String),
}

/// 表示发现的 SQL 文件及其元数据
#[derive(Debug, Clone)]
pub struct SqlFile {
    /// SQL 文件的绝对路径
    pub absolute_path: PathBuf,
    /// 相对于扫描目录的路径
    pub relative_path: String,
    /// JSON 输出将被写入的路径
    pub json_path: PathBuf,
}

impl SqlFile {
    /// 创建新的 SqlFile 实例
    ///
    /// # 参数
    /// * `absolute_path` - SQL 文件的完整路径
    /// * `base_dir` - 用于计算相对路径的基础目录
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

    /// 读取 SQL 文件内容
    pub fn read_content(&self) -> Result<String, ScannerError> {
        fs::read_to_string(&self.absolute_path)
            .map_err(|e| ScannerError::ReadError(format!("{}: {}", self.absolute_path.display(), e)))
    }

    /// 检查对应的 JSON 输出文件是否存在
    pub fn json_exists(&self) -> bool {
        self.json_path.exists()
    }

    /// 获取 JSON 输出文件的最后修改时间
    pub fn json_modified_time(&self) -> Option<std::time::SystemTime> {
        fs::metadata(&self.json_path)
            .ok()
            .and_then(|m| m.modified().ok())
    }
}

/// SQL 文件目录扫描器
pub struct Scanner {
    base_directory: PathBuf,
}

impl Scanner {
    /// 为指定目录创建新的扫描器
    ///
    /// # 参数
    /// * `base_directory` - 要扫描 SQL 文件的目录
    pub fn new<P: AsRef<Path>>(base_directory: P) -> Result<Self, ScannerError> {
        let base_directory = base_directory.as_ref().to_path_buf();
        if !base_directory.exists() {
            return Err(ScannerError::DirectoryNotFound(
                base_directory.display().to_string(),
            ));
        }
        Ok(Self { base_directory })
    }

    /// 扫描目录树中的 SQL 文件
    ///
    /// 递归查找基础目录及子目录中的所有 `.sql` 文件。
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
        
        // 创建测试 SQL 文件
        fs::write(sql_dir.join("test1.sql"), "SELECT 1").unwrap();
        fs::write(sql_dir.join("test2.SQL"), "SELECT 2").unwrap();
        fs::write(sql_dir.join("not_sql.txt"), "not sql").unwrap();
        
        // 创建包含 SQL 文件的子目录
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

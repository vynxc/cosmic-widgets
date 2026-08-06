use std::collections::BTreeSet;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;
use zip::ZipArchive;

use crate::Manifest;

/// Resource ceilings applied before a package is trusted.
#[derive(Debug, Clone, Copy)]
pub struct PackageLimits {
    pub max_files: usize,
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
    pub max_manifest_bytes: u64,
}

impl Default for PackageLimits {
    fn default() -> Self {
        Self {
            max_files: 128,
            max_file_bytes: 8 * 1024 * 1024,
            max_total_bytes: 32 * 1024 * 1024,
            max_manifest_bytes: 64 * 1024,
        }
    }
}

/// Metadata returned after all archive and manifest checks pass.
#[derive(Debug, Clone)]
pub struct ValidatedPackage {
    pub path: PathBuf,
    pub manifest: Manifest,
    pub sha256: String,
    pub files: BTreeSet<String>,
}

/// Package validation error.
#[derive(Debug, Error)]
pub enum PackageError {
    #[error("unable to read package: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid ZIP archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("widget.toml is missing")]
    MissingManifest,
    #[error("invalid widget.toml: {0}")]
    Manifest(#[from] toml::de::Error),
    #[error("manifest validation failed: {0}")]
    InvalidManifest(String),
    #[error("unsafe archive path: {0}")]
    UnsafePath(String),
    #[error("package contains a symbolic link: {0}")]
    Symlink(String),
    #[error("package has too many files")]
    TooManyFiles,
    #[error("package file is too large: {0}")]
    FileTooLarge(String),
    #[error("package expands beyond the total size limit")]
    PackageTooLarge,
    #[error("required package file is missing: {0}")]
    MissingFile(String),
    #[error("HTML contains forbidden active content: {0}")]
    ActiveContent(String),
}

/// Validates a `.cwidget` archive without extracting it.
///
/// # Errors
///
/// Returns [`PackageError`] when the archive cannot be read, violates a resource
/// ceiling, contains an unsafe path or active content, or has an invalid manifest.
pub fn validate_package(
    path: &Path,
    limits: PackageLimits,
) -> Result<ValidatedPackage, PackageError> {
    let mut raw = Vec::new();
    File::open(path)?.read_to_end(&mut raw)?;
    let sha256 = format!("{:x}", Sha256::digest(&raw));
    let cursor = std::io::Cursor::new(raw);
    let mut archive = ZipArchive::new(cursor)?;
    let files = inspect_archive(&mut archive, limits)?;

    let manifest_text = read_limited(&mut archive, "widget.toml", limits.max_manifest_bytes)?
        .ok_or(PackageError::MissingManifest)?;
    let manifest: Manifest = toml::from_str(&manifest_text)?;
    manifest.validate().map_err(PackageError::InvalidManifest)?;

    require_file(&files, &manifest.entry)?;
    if let Some(wasm) = &manifest.wasm {
        require_file(&files, &wasm.module)?;
    }
    validate_html(&mut archive, &manifest.entry, limits.max_file_bytes)?;

    Ok(ValidatedPackage {
        path: path.to_path_buf(),
        manifest,
        sha256,
        files,
    })
}

fn inspect_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    limits: PackageLimits,
) -> Result<BTreeSet<String>, PackageError> {
    if archive.len() > limits.max_files {
        return Err(PackageError::TooManyFiles);
    }
    let mut total = 0_u64;
    let mut files = BTreeSet::new();
    for index in 0..archive.len() {
        let file = archive.by_index(index)?;
        let name = file.name().to_owned();
        if file.enclosed_name().is_none() || name.starts_with('/') || name.contains('\0') {
            return Err(PackageError::UnsafePath(name));
        }
        if file
            .unix_mode()
            .is_some_and(|mode| mode & 0o170_000 == 0o120_000)
        {
            return Err(PackageError::Symlink(name));
        }
        if file.is_dir() {
            continue;
        }
        if file.size() > limits.max_file_bytes {
            return Err(PackageError::FileTooLarge(name));
        }
        total = total.saturating_add(file.size());
        if total > limits.max_total_bytes {
            return Err(PackageError::PackageTooLarge);
        }
        files.insert(name);
    }
    Ok(files)
}

fn read_limited<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
    limit: u64,
) -> Result<Option<String>, PackageError> {
    let Ok(mut file) = archive.by_name(name) else {
        return Ok(None);
    };
    if file.size() > limit {
        return Err(PackageError::FileTooLarge(name.into()));
    }
    let mut value = String::new();
    file.read_to_string(&mut value)?;
    Ok(Some(value))
}

fn require_file(files: &BTreeSet<String>, name: &str) -> Result<(), PackageError> {
    if files.contains(name) {
        Ok(())
    } else {
        Err(PackageError::MissingFile(name.into()))
    }
}

fn validate_html<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    entry: &str,
    limit: u64,
) -> Result<(), PackageError> {
    let html = read_limited(archive, entry, limit)?
        .ok_or_else(|| PackageError::MissingFile(entry.into()))?;
    let lowercase = html.to_ascii_lowercase();
    for forbidden in ["<script", "<iframe", "javascript:", "<object", "<embed"] {
        if lowercase.contains(forbidden) {
            return Err(PackageError::ActiveContent(forbidden.into()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;
    use zip::write::SimpleFileOptions;

    use super::*;

    fn package_with_html(html: &str) -> NamedTempFile {
        let file = NamedTempFile::new().expect("temporary package should be created");
        let writer = file.reopen().expect("temporary package should reopen");
        let mut zip = zip::ZipWriter::new(writer);
        zip.start_file("widget.toml", SimpleFileOptions::default())
            .expect("manifest entry should start");
        write!(
            zip,
            "schema=1\nid='io.github.vynxc.Test'\nversion='0.1.0'\nname='Test'\ndefault_size={{width=200,height=100}}\nmin_size={{width=100,height=50}}\nmax_size={{width=400,height=200}}\n"
        )
        .expect("manifest should write");
        zip.start_file("index.html", SimpleFileOptions::default())
            .expect("HTML entry should start");
        zip.write_all(html.as_bytes()).expect("HTML should write");
        zip.finish().expect("archive should finish");
        file
    }

    #[test]
    fn validate_package_should_accept_declarative_widget() {
        let file = package_with_html("<main data-cw-text='clock.time'></main>");
        assert!(validate_package(file.path(), PackageLimits::default()).is_ok());
    }

    #[test]
    fn validate_package_should_reject_script() {
        let file = package_with_html("<script>alert(1)</script>");
        assert!(matches!(
            validate_package(file.path(), PackageLimits::default()),
            Err(PackageError::ActiveContent(_))
        ));
    }
}

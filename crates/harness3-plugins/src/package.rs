use harness3_domain::{PResult, fault};
use harness3_plugin_protocol::{Manifest, package_path};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn read(path: &Path, limit: u64) -> PResult<Vec<u8>> {
    if !std::fs::symlink_metadata(path)
        .map_err(|_| fault(400, "plugin_file_unreadable"))?
        .file_type()
        .is_file()
    {
        return Err(fault(400, "plugin_file_must_be_regular"));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| fault(400, "plugin_file_unreadable"))?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| fault(400, "plugin_file_unreadable"))?;
    if bytes.len() as u64 > limit {
        return Err(fault(400, "plugin_package_too_large"));
    }
    Ok(bytes)
}
fn file(root: &Path, relative: &str) -> PResult<PathBuf> {
    if !package_path(relative) {
        return Err(fault(400, "invalid_plugin_path"));
    }
    let mut path = root.to_owned();
    for part in relative.split('/') {
        path.push(part);
        if std::fs::symlink_metadata(&path)
            .map_err(|_| fault(400, "plugin_file_unreadable"))?
            .file_type()
            .is_symlink()
        {
            return Err(fault(400, "plugin_symlink_denied"));
        }
    }
    Ok(path)
}
pub fn prepare(cache: &Path, source: &Path) -> PResult<(Manifest, String)> {
    if !source.is_absolute() {
        return Err(fault(400, "plugin_source_must_be_absolute"));
    }
    let root = source
        .canonicalize()
        .map_err(|_| fault(400, "plugin_source_unreadable"))?;
    let manifest: Manifest =
        serde_json::from_slice(&read(&root.join("manifest.json"), 128 * 1024)?)
            .map_err(|_| fault(400, "invalid_plugin_manifest"))?;
    manifest
        .validate()
        .map_err(|_| fault(400, "invalid_plugin_manifest"))?;
    let mut files = Vec::new();
    let mut total = 0;
    for (path, hash) in &manifest.files {
        let bytes = read(&file(&root, path)?, 8 * 1024 * 1024)?;
        total += bytes.len();
        if total > 8 * 1024 * 1024 {
            return Err(fault(400, "plugin_package_too_large"));
        }
        if &digest(&bytes) != hash {
            return Err(fault(400, "plugin_digest_mismatch"));
        }
        files.push((path, bytes));
    }
    let manifest_bytes = serde_json::to_vec(&manifest)?;
    let hash = digest(&manifest_bytes);
    let destination = cache.join(&hash);
    if destination.exists() {
        verify(cache, &hash, &manifest)?;
        return Ok((manifest, hash));
    }
    std::fs::create_dir_all(cache).map_err(|_| fault(500, "plugin_cache_unwritable"))?;
    let staging = cache.join(format!(".stage-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&staging).map_err(|_| fault(500, "plugin_cache_unwritable"))?;
    let outcome = (|| {
        for (path, bytes) in files {
            let target = staging.join(path);
            std::fs::create_dir_all(target.parent().unwrap())
                .map_err(|_| fault(500, "plugin_cache_unwritable"))?;
            std::fs::write(&target, bytes).map_err(|_| fault(500, "plugin_cache_unwritable"))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(
                    &target,
                    std::fs::Permissions::from_mode(if *path == manifest.runtime.entry {
                        0o500
                    } else {
                        0o400
                    }),
                )
                .map_err(|_| fault(500, "plugin_cache_unwritable"))?;
            }
        }
        std::fs::write(staging.join("manifest.json"), &manifest_bytes)
            .map_err(|_| fault(500, "plugin_cache_unwritable"))?;
        match std::fs::rename(&staging, &destination) {
            Ok(()) => Ok(()),
            Err(_) if destination.exists() => verify(cache, &hash, &manifest).map(|_| ()),
            Err(_) => Err(fault(500, "plugin_cache_unwritable")),
        }
    })();
    let _ = std::fs::remove_dir_all(&staging);
    outcome?;
    Ok((manifest, hash))
}
pub fn verify(cache: &Path, hash: &str, manifest: &Manifest) -> PResult<PathBuf> {
    if digest(&serde_json::to_vec(manifest)?) != hash {
        return Err(fault(409, "plugin_snapshot_digest_mismatch"));
    }
    let root = cache.join(hash);
    if std::fs::symlink_metadata(&root)
        .map_err(|_| fault(409, "plugin_cache_missing"))?
        .file_type()
        .is_symlink()
    {
        return Err(fault(409, "plugin_cache_tampered"));
    }
    if read(&root.join("manifest.json"), 128 * 1024)? != serde_json::to_vec(manifest)? {
        return Err(fault(409, "plugin_cache_tampered"));
    }
    for (path, hash) in &manifest.files {
        if &digest(&read(&file(&root, path)?, 8 * 1024 * 1024)?) != hash {
            return Err(fault(409, "plugin_cache_tampered"));
        }
    }
    Ok(root)
}

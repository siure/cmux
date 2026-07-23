use anyhow::{anyhow, bail, Context, Result};
use flate2::read::GzDecoder;
use reqwest::blocking::{Client, Response};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use tar::EntryType;
use url::Url;

const DEFAULT_UPDATE_API_URL: &str =
    "https://api.github.com/repos/manaflow-ai/cmux/releases/latest";
const MAX_RELEASE_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_CHECKSUM_RESPONSE_BYTES: usize = 16 * 1024;
const MAX_UPDATE_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_UPDATE_ARCHIVE_ENTRIES: usize = 20_000;

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReleaseVersion {
    major: u64,
    minor: u64,
    patch: u64,
    prerelease: Option<String>,
}

pub(crate) fn check_for_updates() -> Result<Value> {
    let endpoint = update_api_url()?;
    let allow_http_assets = endpoint.scheme() == "http";
    let (current_version, current_version_source) = installed_version();
    check_for_updates_at(
        &endpoint,
        &current_version,
        &current_version_source,
        linux_update_arch(),
        allow_http_assets,
    )
}

pub(crate) fn update_status_text(status: &Value) -> String {
    let current = status
        .get("current_version")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let latest = status
        .get("latest_version")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let state = status
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let mut lines = vec![
        format!("Current Linux version: {current}"),
        format!("Latest Linux version:  {latest}"),
    ];
    match state {
        "update_available" => lines.push("A Linux update is available.".to_string()),
        "current" => lines.push("cmux Linux is up to date.".to_string()),
        "newer_than_latest" => {
            lines.push("This cmux Linux build is newer than the latest stable release.".to_string())
        }
        _ => lines.push(
            "The current build version cannot be compared with the latest stable release."
                .to_string(),
        ),
    }
    if let Some(url) = status.get("release_url").and_then(Value::as_str) {
        lines.push(format!("Release: {url}"));
    }
    if let Some(url) = status.get("archive_url").and_then(Value::as_str) {
        lines.push(format!("Archive: {url}"));
    }
    if let Some(sha256) = status.get("archive_sha256").and_then(Value::as_str) {
        lines.push(format!("SHA-256: {sha256}"));
    }
    lines.join("\n")
}

pub(crate) fn install_checked_update(
    status: &Value,
    prefix: Option<&Path>,
    force: bool,
) -> Result<Value> {
    install_checked_update_in(status, prefix, force, &update_cache_dir())
}

fn install_checked_update_in(
    status: &Value,
    prefix: Option<&Path>,
    force: bool,
    cache_root: &Path,
) -> Result<Value> {
    if status.get("installable").and_then(Value::as_bool) != Some(true) {
        bail!("the latest Linux release is not installable");
    }
    if !force && status.get("update_available").and_then(Value::as_bool) != Some(true) {
        bail!("no newer stable Linux release is available; pass --force to reinstall");
    }
    let architecture = status
        .get("architecture")
        .and_then(Value::as_str)
        .context("update status is missing architecture")?;
    let archive_name = status
        .get("archive_name")
        .and_then(Value::as_str)
        .context("update status is missing archive_name")?;
    let expected_archive_name = format!("cmux-linux-{architecture}.tar.gz");
    if archive_name != expected_archive_name {
        bail!("update archive name does not match the selected Linux architecture");
    }
    let expected_sha256 = status
        .get("archive_sha256")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .context("update status is missing a valid archive SHA-256")?
        .to_ascii_lowercase();
    let api_url = status
        .get("api_url")
        .and_then(Value::as_str)
        .and_then(|value| Url::parse(value).ok());
    let allow_http = api_url.as_ref().is_some_and(|url| url.scheme() == "http");
    let archive_url = Url::parse(
        status
            .get("archive_url")
            .and_then(Value::as_str)
            .context("update status is missing archive_url")?,
    )
    .context("update archive URL was invalid")?;
    validate_http_url(&archive_url, allow_http)?;
    let latest_version = status
        .get("latest_version")
        .and_then(Value::as_str)
        .context("update status is missing latest_version")?;
    let prefix = update_install_prefix(prefix)?;
    let work = UpdateWorkDir::create(cache_root)?;
    let archive_path = work.path.join(archive_name);
    let downloaded_bytes = download_update_archive(
        &archive_url,
        &archive_path,
        &expected_sha256,
        MAX_UPDATE_ARCHIVE_BYTES,
    )?;
    let extract_root = work.path.join("extract");
    let bundle_name = format!("cmux-linux-{architecture}");
    let bundle_root = extract_update_bundle(&archive_path, &extract_root, &bundle_name)?;
    let installer = bundle_root.join("install.sh");
    let output = Command::new(&installer)
        .env("PREFIX", &prefix)
        .env_remove("CMUX_LINUX_BIN_DIR")
        .env_remove("CMUX_LINUX_BUNDLE_VERSION")
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("failed to run {}", installer.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "Linux update installer failed with {}: {}",
            output.status,
            stderr.trim()
        );
    }
    verify_installed_version(&prefix, latest_version)?;

    let mut result = status.clone();
    result["installed"] = json!(true);
    result["installed_prefix"] = json!(prefix);
    result["downloaded_bytes"] = json!(downloaded_bytes);
    result["installer_output"] = json!(String::from_utf8_lossy(&output.stdout).trim());
    Ok(result)
}

struct UpdateWorkDir {
    path: PathBuf,
}

impl UpdateWorkDir {
    fn create(root: &Path) -> Result<Self> {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(root)
            .with_context(|| format!("failed to create Linux update cache {}", root.display()))?;
        fs::set_permissions(root, fs::Permissions::from_mode(0o700)).with_context(|| {
            format!(
                "failed to secure Linux update cache permissions for {}",
                root.display()
            )
        })?;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        for attempt in 0..32_u32 {
            let path = root.join(format!("install-{}-{nonce}-{attempt}", std::process::id()));
            match fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => {
                    return Err(err).with_context(|| {
                        format!(
                            "failed to create Linux update work directory {}",
                            path.display()
                        )
                    });
                }
            }
        }
        bail!("failed to allocate a unique Linux update work directory")
    }
}

impl Drop for UpdateWorkDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn update_cache_dir() -> PathBuf {
    if let Some(path) = normalized_env("XDG_CACHE_HOME") {
        return Path::new(&path).join("cmux/updates");
    }
    if let Some(home) = normalized_env("HOME") {
        return Path::new(&home).join(".cache/cmux/updates");
    }
    env::temp_dir().join(format!("cmux-{}-updates", std::process::id()))
}

fn update_install_prefix(prefix: Option<&Path>) -> Result<PathBuf> {
    let prefix = prefix
        .map(Path::to_path_buf)
        .or_else(|| normalized_env("CMUX_LINUX_UPDATE_PREFIX").map(PathBuf::from))
        .or_else(installed_prefix)
        .or_else(|| normalized_env("HOME").map(|home| Path::new(&home).join(".local")))
        .context("Linux update install prefix is unavailable; pass --prefix")?;
    if !prefix.is_absolute() {
        bail!("Linux update install prefix must be an absolute path");
    }
    Ok(prefix)
}

fn installed_prefix() -> Option<PathBuf> {
    let executable = env::current_exe().ok()?;
    let prefix = executable.parent()?.parent()?.to_path_buf();
    prefix
        .join("share/cmux/bundle-version")
        .is_file()
        .then_some(prefix)
}

fn download_update_archive(
    url: &Url,
    destination: &Path,
    expected_sha256: &str,
    max_bytes: u64,
) -> Result<u64> {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        .user_agent(format!("cmux-linux/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .context("failed to create Linux update download client")?;
    let mut response = client
        .get(url.clone())
        .send()
        .context("failed to download the cmux Linux update archive")?;
    if !response.status().is_success() {
        bail!("Linux update archive returned HTTP {}", response.status());
    }
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes)
    {
        bail!("Linux update archive exceeded {max_bytes} bytes");
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(destination)
        .with_context(|| {
            format!(
                "failed to create Linux update archive {}",
                destination.display()
            )
        })?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = response
            .read(&mut buffer)
            .context("failed while downloading the cmux Linux update archive")?;
        if count == 0 {
            break;
        }
        downloaded = downloaded.saturating_add(count as u64);
        if downloaded > max_bytes {
            bail!("Linux update archive exceeded {max_bytes} bytes");
        }
        hasher.update(&buffer[..count]);
        file.write_all(&buffer[..count])
            .context("failed to write the cmux Linux update archive")?;
    }
    file.sync_all()
        .context("failed to flush the cmux Linux update archive")?;
    let actual_sha256 = format!("{:x}", hasher.finalize());
    if actual_sha256 != expected_sha256 {
        bail!(
            "Linux update archive SHA-256 mismatch: expected {expected_sha256}, got {actual_sha256}"
        );
    }
    Ok(downloaded)
}

fn extract_update_bundle(
    archive_path: &Path,
    extract_root: &Path,
    expected_bundle_name: &str,
) -> Result<PathBuf> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(extract_root)
        .with_context(|| {
            format!(
                "failed to create Linux update extraction directory {}",
                extract_root.display()
            )
        })?;
    let file = File::open(archive_path)
        .with_context(|| format!("failed to open {}", archive_path.display()))?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let mut entries = archive
        .entries()
        .context("failed to read the Linux update archive")?;
    let mut entry_count = 0_usize;
    while let Some(entry) = entries.next() {
        let mut entry = entry.context("failed to read a Linux update archive entry")?;
        entry_count += 1;
        if entry_count > MAX_UPDATE_ARCHIVE_ENTRIES {
            bail!("Linux update archive exceeded {MAX_UPDATE_ARCHIVE_ENTRIES} entries");
        }
        let path = entry
            .path()
            .context("Linux update archive contained an invalid path")?
            .into_owned();
        validate_archive_entry_path(&path, expected_bundle_name)?;
        let entry_type = entry.header().entry_type();
        if !(entry_type.is_file() || entry_type.is_dir() || entry_type.is_symlink()) {
            bail!(
                "Linux update archive contained unsupported entry type for {}",
                path.display()
            );
        }
        if entry_type == EntryType::Symlink {
            let target = entry
                .link_name()
                .context("failed to read Linux update symlink target")?
                .context("Linux update archive symlink was missing its target")?;
            validate_archive_link_target(&target)?;
        }
        if !entry
            .unpack_in(extract_root)
            .with_context(|| format!("failed to extract {}", path.display()))?
        {
            bail!(
                "Linux update archive entry escaped the extraction root: {}",
                path.display()
            );
        }
    }
    if entry_count == 0 {
        bail!("Linux update archive was empty");
    }
    let bundle_root = extract_root.join(expected_bundle_name);
    for required in [
        bundle_root.join("install.sh"),
        bundle_root.join("bin/cmux"),
        bundle_root.join("share/cmux/bundle-version"),
        bundle_root.join("share/cmux/build-provenance.txt"),
    ] {
        if !required.is_file() {
            bail!(
                "Linux update archive is missing required file {}",
                required.display()
            );
        }
    }
    Ok(bundle_root)
}

fn validate_archive_entry_path(path: &Path, expected_bundle_name: &str) -> Result<()> {
    let mut components = path.components();
    match components.next() {
        Some(Component::Normal(root)) if root == expected_bundle_name => {}
        _ => bail!(
            "Linux update archive entry is outside {expected_bundle_name}: {}",
            path.display()
        ),
    }
    for component in components {
        if !matches!(component, Component::Normal(_)) {
            bail!(
                "Linux update archive contained unsafe path {}",
                path.display()
            );
        }
    }
    Ok(())
}

fn validate_archive_link_target(target: &Path) -> Result<()> {
    if target.is_absolute()
        || target
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!(
            "Linux update archive contained unsafe symlink target {}",
            target.display()
        );
    }
    Ok(())
}

fn verify_installed_version(prefix: &Path, expected_version: &str) -> Result<()> {
    let binary = prefix.join("bin/cmux");
    let output = Command::new(&binary)
        .arg("--version")
        .env_remove("CMUX_LINUX_BUNDLE_VERSION")
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("failed to verify installed binary {}", binary.display()))?;
    if !output.status.success() {
        bail!("installed cmux version check failed with {}", output.status);
    }
    let expected = format!("cmux-linux {expected_version}");
    let actual = String::from_utf8_lossy(&output.stdout);
    if actual.trim() != expected {
        bail!(
            "installed cmux version mismatch: expected {expected}, got {}",
            actual.trim()
        );
    }
    Ok(())
}

fn check_for_updates_at(
    endpoint: &Url,
    current_version: &str,
    current_version_source: &str,
    architecture: &str,
    allow_http_assets: bool,
) -> Result<Value> {
    let client = Client::builder()
        .timeout(Duration::from_secs(8))
        .user_agent(format!("cmux-linux/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .context("failed to create Linux update client")?;
    let release_text = bounded_response_text(
        client
            .get(endpoint.clone())
            .send()
            .context("failed to request the latest cmux release")?,
        MAX_RELEASE_RESPONSE_BYTES,
        "latest release response",
    )?;
    let release: Value =
        serde_json::from_str(&release_text).context("latest cmux release response was not JSON")?;
    let latest_version = release
        .get("tag_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("latest cmux release did not include tag_name")?;
    let release_url = validated_release_url(
        release
            .get("html_url")
            .and_then(Value::as_str)
            .context("latest cmux release did not include html_url")?,
        allow_http_assets,
    )?;
    let archive_name = format!("cmux-linux-{architecture}.tar.gz");
    let checksum_name = format!("{archive_name}.sha256");
    let archive_url = release_asset_url(&release, &archive_name, allow_http_assets)?;
    let checksum_url = release_asset_url(&release, &checksum_name, allow_http_assets)?;
    let checksum_text = bounded_response_text(
        client
            .get(checksum_url.clone())
            .send()
            .context("failed to request the Linux archive checksum")?,
        MAX_CHECKSUM_RESPONSE_BYTES,
        "Linux archive checksum response",
    )?;
    let archive_sha256 = parse_checksum_file(&checksum_text, &archive_name)?;

    let comparison = match (
        parse_release_version(current_version),
        parse_release_version(latest_version),
    ) {
        (Some(current), Some(latest)) => Some(current.cmp(&latest)),
        _ => None,
    };
    let (status, update_available) = match comparison {
        Some(Ordering::Less) => ("update_available", Some(true)),
        Some(Ordering::Equal) => ("current", Some(false)),
        Some(Ordering::Greater) => ("newer_than_latest", Some(false)),
        None => ("unknown", None),
    };

    Ok(json!({
        "platform": "linux",
        "channel": "stable",
        "architecture": architecture,
        "current_version": current_version,
        "current_version_source": current_version_source,
        "latest_version": latest_version,
        "status": status,
        "update_available": update_available,
        "installable": true,
        "release_url": release_url.as_str(),
        "archive_name": archive_name,
        "archive_url": archive_url.as_str(),
        "archive_sha256": archive_sha256,
        "checksum_url": checksum_url.as_str(),
        "api_url": endpoint.as_str()
    }))
}

fn update_api_url() -> Result<Url> {
    let overridden = env::var("CMUX_LINUX_UPDATE_API_URL").ok();
    let raw = overridden.as_deref().unwrap_or(DEFAULT_UPDATE_API_URL);
    let url = Url::parse(raw).context("CMUX_LINUX_UPDATE_API_URL must be an absolute URL")?;
    validate_http_url(&url, overridden.is_some())?;
    Ok(url)
}

fn validated_release_url(raw: &str, allow_http: bool) -> Result<Url> {
    let url = Url::parse(raw).context("latest cmux release URL was invalid")?;
    validate_http_url(&url, allow_http)?;
    Ok(url)
}

fn release_asset_url(release: &Value, name: &str, allow_http: bool) -> Result<Url> {
    let assets = release
        .get("assets")
        .and_then(Value::as_array)
        .context("latest cmux release did not include assets")?;
    let raw = assets
        .iter()
        .find(|asset| asset.get("name").and_then(Value::as_str) == Some(name))
        .and_then(|asset| asset.get("browser_download_url"))
        .and_then(Value::as_str)
        .with_context(|| format!("latest cmux release is missing {name}"))?;
    let url = Url::parse(raw).with_context(|| format!("{name} download URL was invalid"))?;
    validate_http_url(&url, allow_http)?;
    Ok(url)
}

fn validate_http_url(url: &Url, allow_http: bool) -> Result<()> {
    if !url.username().is_empty() || url.password().is_some() {
        bail!("Linux update URLs must not contain credentials");
    }
    match url.scheme() {
        "https" => Ok(()),
        "http" if allow_http => Ok(()),
        _ => bail!("Linux update URLs must use HTTPS"),
    }
}

fn bounded_response_text(mut response: Response, max_bytes: usize, label: &str) -> Result<String> {
    let status = response.status();
    if !status.is_success() {
        bail!("{label} returned HTTP {status}");
    }
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        bail!("{label} exceeded {max_bytes} bytes");
    }
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(max_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read {label}"))?;
    if bytes.len() > max_bytes {
        bail!("{label} exceeded {max_bytes} bytes");
    }
    String::from_utf8(bytes).with_context(|| format!("{label} was not UTF-8"))
}

fn parse_checksum_file(text: &str, archive_name: &str) -> Result<String> {
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let mut fields = line.split_whitespace();
        let Some(hash) = fields.next() else {
            continue;
        };
        let Some(name) = fields.next() else {
            continue;
        };
        let name = name.trim_start_matches('*');
        if name == archive_name
            && hash.len() == 64
            && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Ok(hash.to_ascii_lowercase());
        }
    }
    Err(anyhow!(
        "Linux archive checksum did not contain a valid SHA-256 for {archive_name}"
    ))
}

pub(crate) fn installed_version() -> (String, String) {
    if let Some(version) = normalized_env("CMUX_LINUX_BUNDLE_VERSION") {
        return (version, "environment".to_string());
    }
    for path in bundle_version_paths() {
        if let Ok(text) = fs::read_to_string(&path) {
            if let Some(version) = text.lines().map(str::trim).find(|line| !line.is_empty()) {
                return (version.to_string(), path.display().to_string());
            }
        }
    }
    (env!("CARGO_PKG_VERSION").to_string(), "cargo".to_string())
}

fn bundle_version_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(root) = normalized_env("CMUX_LINUX_BUNDLE_ROOT") {
        paths.push(Path::new(&root).join("share/cmux/bundle-version"));
    }
    if let Ok(executable) = env::current_exe() {
        if let Some(prefix) = executable.parent().and_then(Path::parent) {
            paths.push(prefix.join("share/cmux/bundle-version"));
        }
    }
    paths
}

fn normalized_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn linux_update_arch() -> &'static str {
    match env::consts::ARCH {
        "x86_64" | "amd64" => "x86_64",
        "aarch64" | "arm64" => "aarch64",
        other => other,
    }
}

fn parse_release_version(raw: &str) -> Option<ReleaseVersion> {
    let raw = raw.trim().strip_prefix('v').unwrap_or(raw.trim());
    let without_build = raw.split_once('+').map(|(value, _)| value).unwrap_or(raw);
    let (core, prerelease) = without_build
        .split_once('-')
        .map(|(core, prerelease)| (core, Some(prerelease.to_string())))
        .unwrap_or((without_build, None));
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    if parts.next().is_some() || prerelease.as_deref() == Some("") {
        return None;
    }
    Some(ReleaseVersion {
        major,
        minor,
        patch,
        prerelease,
    })
}

impl Ord for ReleaseVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then_with(|| self.minor.cmp(&other.minor))
            .then_with(|| self.patch.cmp(&other.patch))
            .then_with(|| match (&self.prerelease, &other.prerelease) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(left), Some(right)) => left.cmp(right),
            })
    }
}

impl PartialOrd for ReleaseVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use tar::{Builder, Header};

    #[test]
    fn release_versions_compare_stable_and_prerelease_builds() {
        assert!(
            parse_release_version("v1.2.3").unwrap()
                > parse_release_version("1.2.3-beta.1").unwrap()
        );
        assert!(parse_release_version("1.3.0").unwrap() > parse_release_version("1.2.99").unwrap());
        assert_eq!(
            parse_release_version("v1.2").unwrap(),
            parse_release_version("1.2.0").unwrap()
        );
        assert!(parse_release_version("nightly-deadbee").is_none());
    }

    #[test]
    fn checksum_parser_requires_exact_archive_name_and_sha256() {
        let hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(
            parse_checksum_file(
                &format!("{hash}  cmux-linux-x86_64.tar.gz\n"),
                "cmux-linux-x86_64.tar.gz"
            )
            .unwrap(),
            hash
        );
        assert!(parse_checksum_file(
            &format!("{hash}  cmux-linux-aarch64.tar.gz\n"),
            "cmux-linux-x86_64.tar.gz"
        )
        .is_err());
        assert!(parse_checksum_file(
            "bad  cmux-linux-x86_64.tar.gz\n",
            "cmux-linux-x86_64.tar.gz"
        )
        .is_err());
    }

    #[test]
    fn update_status_text_reports_download_metadata() {
        let text = update_status_text(&json!({
            "current_version": "v1.0.0",
            "latest_version": "v1.1.0",
            "status": "update_available",
            "release_url": "https://example.test/releases/v1.1.0",
            "archive_url": "https://example.test/cmux-linux-x86_64.tar.gz",
            "archive_sha256": "abc"
        }));
        assert!(text.contains("A Linux update is available."));
        assert!(text.contains("Archive: https://example.test/cmux-linux-x86_64.tar.gz"));
    }

    #[test]
    fn update_check_fetches_release_and_validates_checksum_asset() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind update fixture");
        let address = listener.local_addr().expect("update fixture address");
        let hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let release = json!({
            "tag_name": "v1.2.0",
            "html_url": format!("http://{address}/release"),
            "assets": [
                {
                    "name": "cmux-linux-x86_64.tar.gz",
                    "browser_download_url": format!("http://{address}/archive")
                },
                {
                    "name": "cmux-linux-x86_64.tar.gz.sha256",
                    "browser_download_url": format!("http://{address}/checksum")
                }
            ]
        })
        .to_string();
        let checksum = format!("{hash}  cmux-linux-x86_64.tar.gz\n");
        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().expect("accept update request");
                let mut request = [0_u8; 4096];
                let count = stream.read(&mut request).expect("read update request");
                let request = String::from_utf8_lossy(&request[..count]);
                let body = if request.starts_with("GET /latest ") {
                    release.as_str()
                } else if request.starts_with("GET /checksum ") {
                    checksum.as_str()
                } else {
                    panic!("unexpected update request: {request}");
                };
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .expect("write update response");
            }
        });
        let status = check_for_updates_at(
            &Url::parse(&format!("http://{address}/latest")).unwrap(),
            "v1.1.0",
            "test",
            "x86_64",
            true,
        )
        .expect("update status");
        server.join().expect("update fixture");

        assert_eq!(status["status"], "update_available");
        assert_eq!(status["update_available"], true);
        assert_eq!(status["archive_sha256"], hash);
        assert_eq!(status["current_version_source"], "test");
    }

    #[test]
    fn checked_update_downloads_verifies_extracts_and_installs_bundle() {
        let version = "v1.2.0";
        let archive = update_bundle_fixture(version);
        let archive_sha256 = format!("{:x}", Sha256::digest(&archive));
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind archive fixture");
        let address = listener.local_addr().expect("archive fixture address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept archive request");
            let mut request = [0_u8; 4096];
            let count = stream.read(&mut request).expect("read archive request");
            assert!(
                String::from_utf8_lossy(&request[..count]).starts_with("GET /archive "),
                "unexpected archive request"
            );
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/gzip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                archive.len()
            )
            .expect("write archive headers");
            stream.write_all(&archive).expect("write archive body");
        });
        let temp = tempfile::tempdir().expect("update install tempdir");
        let prefix = temp.path().join("prefix");
        let cache = temp.path().join("cache");
        let status = json!({
            "architecture": "x86_64",
            "archive_name": "cmux-linux-x86_64.tar.gz",
            "archive_url": format!("http://{address}/archive"),
            "archive_sha256": archive_sha256,
            "api_url": format!("http://{address}/latest"),
            "latest_version": version,
            "update_available": true,
            "installable": true
        });

        let installed = install_checked_update_in(&status, Some(&prefix), false, &cache)
            .expect("install checked update");
        server.join().expect("archive fixture");

        assert_eq!(installed["installed"], true);
        assert_eq!(installed["latest_version"], version);
        assert_eq!(
            fs::read_to_string(prefix.join("share/cmux/bundle-version"))
                .unwrap()
                .trim(),
            version
        );
        let version_output = Command::new(prefix.join("bin/cmux"))
            .arg("--version")
            .output()
            .expect("run installed fixture");
        assert_eq!(
            String::from_utf8(version_output.stdout).unwrap().trim(),
            format!("cmux-linux {version}")
        );
        assert!(
            fs::read_dir(&cache).unwrap().next().is_none(),
            "temporary update work directory was not removed"
        );
    }

    #[test]
    fn update_archive_paths_and_links_reject_escape_attempts() {
        assert!(validate_archive_entry_path(
            Path::new("cmux-linux-x86_64/bin/cmux"),
            "cmux-linux-x86_64"
        )
        .is_ok());
        assert!(validate_archive_entry_path(
            Path::new("../cmux-linux-x86_64/bin/cmux"),
            "cmux-linux-x86_64"
        )
        .is_err());
        assert!(
            validate_archive_entry_path(Path::new("other-root/bin/cmux"), "cmux-linux-x86_64")
                .is_err()
        );
        assert!(validate_archive_link_target(Path::new("libghostty-vt.so.0")).is_ok());
        assert!(validate_archive_link_target(Path::new("../outside")).is_err());
        assert!(validate_archive_link_target(Path::new("/tmp/outside")).is_err());
    }

    #[test]
    fn checked_update_rejects_archive_checksum_mismatch_before_extraction() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind tampered archive fixture");
        let address = listener.local_addr().expect("tampered archive address");
        let archive = b"tampered archive bytes".to_vec();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept tampered archive request");
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request).expect("read tampered request");
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                archive.len()
            )
            .expect("write tampered headers");
            stream.write_all(&archive).expect("write tampered archive");
        });
        let temp = tempfile::tempdir().expect("tampered update tempdir");
        let prefix = temp.path().join("prefix");
        let status = json!({
            "architecture": "x86_64",
            "archive_name": "cmux-linux-x86_64.tar.gz",
            "archive_url": format!("http://{address}/archive"),
            "archive_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
            "api_url": format!("http://{address}/latest"),
            "latest_version": "v1.2.0",
            "update_available": true,
            "installable": true
        });

        let error =
            install_checked_update_in(&status, Some(&prefix), false, &temp.path().join("cache"))
                .expect_err("tampered archive must fail");
        server.join().expect("tampered archive fixture");

        assert!(
            error.to_string().contains("SHA-256 mismatch"),
            "unexpected checksum error: {error}"
        );
        assert!(!prefix.exists(), "tampered archive reached installer");
    }

    fn update_bundle_fixture(version: &str) -> Vec<u8> {
        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut archive = Builder::new(encoder);
        let installer = r#"#!/bin/sh
set -eu
root=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
install -d "$PREFIX/bin" "$PREFIX/share/cmux"
install -m 0755 "$root/bin/cmux" "$PREFIX/bin/cmux"
install -m 0644 "$root/share/cmux/bundle-version" "$PREFIX/share/cmux/bundle-version"
install -m 0644 "$root/share/cmux/build-provenance.txt" "$PREFIX/share/cmux/build-provenance.txt"
printf 'fixture update installed\n'
"#;
        append_archive_file(
            &mut archive,
            "cmux-linux-x86_64/install.sh",
            installer.as_bytes(),
            0o755,
        );
        append_archive_file(
            &mut archive,
            "cmux-linux-x86_64/bin/cmux",
            format!("#!/bin/sh\nprintf 'cmux-linux {version}\\n'\n").as_bytes(),
            0o755,
        );
        append_archive_file(
            &mut archive,
            "cmux-linux-x86_64/share/cmux/bundle-version",
            format!("{version}\n").as_bytes(),
            0o644,
        );
        append_archive_file(
            &mut archive,
            "cmux-linux-x86_64/share/cmux/build-provenance.txt",
            format!("schema=cmux.linux-bundle.provenance.v1\nversion={version}\n").as_bytes(),
            0o644,
        );
        let encoder = archive.into_inner().expect("finish update tar");
        encoder.finish().expect("finish update gzip")
    }

    fn append_archive_file(
        archive: &mut Builder<GzEncoder<Vec<u8>>>,
        path: &str,
        contents: &[u8],
        mode: u32,
    ) {
        let mut header = Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(mode);
        header.set_mtime(1_700_000_000);
        header.set_cksum();
        archive
            .append_data(&mut header, path, contents)
            .expect("append update fixture file");
    }
}

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

const GTK4_DEVELOPMENT_PACKAGE_HINT: &str = "Install GTK4 development files: Fedora/RHEL `sudo dnf install gtk4-devel pkgconf-pkg-config`; Debian/Ubuntu `sudo apt install libgtk-4-dev pkg-config`; Arch `sudo pacman -S gtk4 pkgconf`; openSUSE `sudo zypper install gtk4-devel pkgconf-pkg-config`.";
const WEBKITGTK_DEVELOPMENT_PACKAGE_HINT: &str = "Install WebKitGTK 6 development files: Fedora/RHEL `sudo dnf install webkitgtk6.0-devel`; Debian/Ubuntu `sudo apt install libwebkitgtk-6.0-dev`; Arch `sudo pacman -S webkitgtk-6.0`; openSUSE `sudo zypper install webkitgtk-6_0-devel`.";

fn main() {
    println!("cargo:rerun-if-env-changed=PKG_CONFIG");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_LIBDIR");
    println!("cargo:rerun-if-env-changed=LD_LIBRARY_PATH");
    println!("cargo:rerun-if-env-changed=LIBRARY_PATH");
    println!("cargo:rerun-if-changed=webkit/request_headers_extension.c");
    println!("cargo:rerun-if-changed=resources/ui/cmux.gresource.xml");
    println!("cargo:rerun-if-changed=resources/ui/css/cmux.css");
    println!("cargo:rerun-if-changed=resources/ui/css/tokens.css");
    println!("cargo:rerun-if-changed=resources/ui/css/legacy.css");
    println!("cargo:rerun-if-changed=resources/ui/css/next.css");
    println!("cargo:rerun-if-changed=resources/ui/css/parity-sidebar.css");
    println!("cargo:rerun-if-changed=resources/ui/css/parity-panes.css");
    println!("cargo:rerun-if-changed=resources/ui/css/parity-panels.css");
    println!("cargo:rerun-if-changed=resources/ui/css/parity-overlays.css");
    println!("cargo:rerun-if-changed=resources/ui/strings/en.json");
    println!("cargo:rerun-if-changed=resources/ui/strings/ja.json");

    if env::var_os("CARGO_FEATURE_GTK").is_none()
        || env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux")
    {
        return;
    }

    if !pkg_config_has_gtk4() {
        fail("gtk4.pc was not found");
    }

    if gtk4_link_library().is_none() {
        fail(
            "gtk4.pc was found, but libgtk-4.so was not found on pkg-config or common linker paths",
        );
    }
    compile_webkit_request_headers_extension();
    compile_gtk_resources();
}

fn fail(reason: &str) -> ! {
    eprintln!("error: {reason}. {GTK4_DEVELOPMENT_PACKAGE_HINT}");
    std::process::exit(1);
}

fn compile_gtk_resources() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    let target = out_dir.join("cmux-ui.gresource");
    let compiler =
        env::var_os("GLIB_COMPILE_RESOURCES").unwrap_or_else(|| "glib-compile-resources".into());
    let status = Command::new(compiler)
        .args([
            "--sourcedir",
            "resources/ui",
            "--target",
            target.to_str().expect("UTF-8 GTK resource output path"),
            "resources/ui/cmux.gresource.xml",
        ])
        .status()
        .unwrap_or_else(|err| fail(&format!("failed to run glib-compile-resources: {err}")));
    if !status.success() {
        fail("failed to compile GTK resources");
    }
}

fn compile_webkit_request_headers_extension() {
    const PACKAGE: &str = "webkitgtk-web-process-extension-6.0";
    let pkg_config = env::var_os("PKG_CONFIG").unwrap_or_else(|| "pkg-config".into());
    let output = Command::new(&pkg_config)
        .args(["--cflags", "--libs", PACKAGE])
        .output()
        .unwrap_or_else(|err| {
            fail_webkit(&format!("failed to run pkg-config for {PACKAGE}: {err}"))
        });
    if !output.status.success() {
        fail_webkit(&format!("{PACKAGE}.pc was not found"));
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    let extension = out_dir.join("libcmux-webkit-request-headers.so");
    let compiler = env::var_os("CC").unwrap_or_else(|| "cc".into());
    let mut command = Command::new(compiler);
    command.args([
        "-shared",
        "-fPIC",
        "-O2",
        "-o",
        extension.to_str().expect("UTF-8 extension output path"),
        "webkit/request_headers_extension.c",
    ]);
    command.args(String::from_utf8_lossy(&output.stdout).split_whitespace());
    let status = command.status().unwrap_or_else(|err| {
        fail_webkit(&format!("failed to compile web process extension: {err}"))
    });
    if !status.success() {
        fail_webkit("failed to compile the WebKitGTK web process extension");
    }
    println!(
        "cargo:rustc-env=CMUX_WEBKIT_EXTENSION_PATH={}",
        extension.display()
    );
}

fn fail_webkit(reason: &str) -> ! {
    eprintln!("error: {reason}. {WEBKITGTK_DEVELOPMENT_PACKAGE_HINT}");
    std::process::exit(1);
}

fn pkg_config_has_gtk4() -> bool {
    let pkg_config = env::var_os("PKG_CONFIG").unwrap_or_else(|| "pkg-config".into());
    Command::new(pkg_config)
        .arg("--exists")
        .arg("gtk4")
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn gtk4_link_library() -> Option<PathBuf> {
    gtk4_library_dirs()
        .into_iter()
        .map(|dir| dir.join("libgtk-4.so"))
        .find(|path| path.exists())
}

fn gtk4_library_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    dirs.extend(pkg_config_library_dirs("gtk4"));
    dirs.extend(env_path_dirs("LIBRARY_PATH"));
    dirs.extend(env_path_dirs("LD_LIBRARY_PATH"));
    dirs.extend(
        [
            "/usr/lib64",
            "/usr/lib",
            "/usr/local/lib64",
            "/usr/local/lib",
            "/lib64",
            "/lib",
            "/usr/lib/x86_64-linux-gnu",
            "/usr/lib/aarch64-linux-gnu",
        ]
        .iter()
        .map(PathBuf::from),
    );
    dedupe_paths(dirs)
}

fn pkg_config_library_dirs(package: &str) -> Vec<PathBuf> {
    let pkg_config = env::var_os("PKG_CONFIG").unwrap_or_else(|| "pkg-config".into());
    let Ok(output) = Command::new(pkg_config)
        .arg("--libs-only-L")
        .arg(package)
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    parse_pkg_config_library_dirs(&String::from_utf8_lossy(&output.stdout))
}

fn parse_pkg_config_library_dirs(output: &str) -> Vec<PathBuf> {
    output
        .split_whitespace()
        .filter_map(|token| token.strip_prefix("-L"))
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn env_path_dirs(name: &str) -> Vec<PathBuf> {
    env::var_os(name)
        .map(|value| env::split_paths(&value).collect())
        .unwrap_or_default()
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut deduped: Vec<PathBuf> = Vec::new();
    for path in paths {
        if path.as_os_str().is_empty() || deduped.iter().any(|seen| same_path(seen, &path)) {
            continue;
        }
        deduped.push(path);
    }
    deduped
}

fn same_path(left: &Path, right: &Path) -> bool {
    left == right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pkg_config_library_dirs_reads_l_flags() {
        assert_eq!(
            parse_pkg_config_library_dirs("-L/opt/gtk/lib -lgtk-4 -L/usr/lib64"),
            vec![PathBuf::from("/opt/gtk/lib"), PathBuf::from("/usr/lib64")]
        );
    }

    #[test]
    fn dedupe_paths_keeps_first_nonempty_path() {
        assert_eq!(
            dedupe_paths(vec![
                PathBuf::new(),
                PathBuf::from("/usr/lib"),
                PathBuf::from("/usr/lib"),
                PathBuf::from("/opt/lib"),
            ]),
            vec![PathBuf::from("/usr/lib"), PathBuf::from("/opt/lib")]
        );
    }
}

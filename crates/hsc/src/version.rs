//! hsc's version, and the GHC release it stands in for where cabal and package code need one.
//!
//! cabal only builds with compilers it knows, so it drives hsc as GHC (through the `ghc` link
//! that `--install-ghc-shims` makes). It reads the version from `ghc --numeric-version` and picks
//! its flags by it, requires `ghc-pkg --version` to match, and names the `.ghc.environment.*`
//! files it writes after it. Package code tests `__GLASGOW_HASKELL__`. Those places report
//! [`GHC_COMPAT_VERSION`]. The compiler banner reports hsc's own [`VERSION`].

use std::path::{Path, PathBuf};

/// hsc's own version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The GHC release hsc is compatible with, which it reports where cabal or package code expects
/// a GHC version.
pub const GHC_COMPAT_VERSION: &str = "9.12.2";

/// The platform part of version-specific directory and file names.
const PLATFORM: &str = "x86_64-linux";

/// [`GHC_COMPAT_VERSION`] as major, minor and patch level.
pub fn ghc_compat_parts() -> [u32; 3] {
    let mut parts = GHC_COMPAT_VERSION.split('.');
    [(); 3].map(|_| {
        parts
            .next()
            .and_then(|p| p.parse().ok())
            .expect("major.minor.patch version")
    })
}

/// The value of `__GLASGOW_HASKELL__`: major * 100 + minor (`912`).
pub fn ghc_compat_int() -> u32 {
    let [major, minor, _] = ghc_compat_parts();
    major * 100 + minor
}

/// The name of the package environment file cabal writes into a project
/// (`.ghc.environment.x86_64-linux-9.12.2`).
pub fn ghc_environment_file_name() -> String {
    format!(".ghc.environment.{PLATFORM}-{GHC_COMPAT_VERSION}")
}

/// hsc's per-user directory (`~/.hsc/x86_64-linux-<hsc version>`), which holds the user package
/// database and named package environments. Fall back to existing entries in the old directory.
pub(crate) fn user_path(relative: &Path) -> Option<PathBuf> {
    let root = PathBuf::from(std::env::var_os("HOME")?).join(".hsc");
    let current = root.join(format!("{PLATFORM}-{VERSION}")).join(relative);
    let legacy = root
        .join(format!("{PLATFORM}-{GHC_COMPAT_VERSION}"))
        .join(relative);
    if !current.exists() && legacy.exists() {
        return Some(legacy);
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compat_version_parts() {
        assert_eq!(ghc_compat_parts(), [9, 12, 2]);
        assert_eq!(ghc_compat_int(), 912);
        assert_eq!(
            ghc_environment_file_name(),
            ".ghc.environment.x86_64-linux-9.12.2"
        );
    }
}

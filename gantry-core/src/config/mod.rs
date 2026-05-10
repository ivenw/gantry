pub mod credentials;
pub mod project;
pub mod provider;

pub use credentials::{
    ApiKeyCredential, Credential, CredentialsCatalog, CredentialsRepository, OauthCredential,
    StoredCredential,
};
pub use project::ProjectConfig;
pub use provider::{
    CopilotProviderConfig, CortecsProviderConfig, OllamaProviderConfig,
    OpenAiCompletionsProviderConfig, OpenAiResponsesProviderConfig, ProviderConfig,
    ProviderConfigCatalog, ProviderConfigRepository,
};

use std::path::Path;

use anyhow::{Context, Result};

/// Writes `contents` to `path` atomically via a sibling temp file and rename.
///
/// Creates parent directories if they do not exist.
pub(super) fn atomic_write(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, contents).with_context(|| format!("failed to write {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("failed to rename {} to {}", tmp.display(), path.display()))?;
    Ok(())
}

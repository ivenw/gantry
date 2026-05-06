use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::provider::ProviderAlias;

/// The full set of credentials, keyed by provider alias.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct CredentialsCatalog(pub HashMap<ProviderAlias, Credential>);

impl CredentialsCatalog {
    /// Returns the credential for the given provider alias, if present.
    pub fn get(&self, alias: &ProviderAlias) -> Option<&Credential> {
        self.0.get(alias)
    }
}

/// A resolved credential for a provider instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Credential {
    /// A literal API key value stored directly in the credentials file.
    ApiKey { value: String },
    /// An API key resolved from the named environment variable at the point of use.
    ApiKeyEnv { var: String },
    /// An OAuth token set managed by the application.
    OauthToken {
        access_token: String,
        refresh_token: String,
        expires_at: String,
    },
}

impl Credential {
    /// Resolves the credential to an API key string.
    ///
    /// Returns an error if the credential is not an API key variant, or if the
    /// referenced environment variable is unset.
    pub fn resolve_api_key(&self) -> anyhow::Result<String> {
        match self {
            Credential::ApiKey { value } => Ok(value.clone()),
            Credential::ApiKeyEnv { var } => std::env::var(var).map_err(|_| {
                anyhow::anyhow!("environment variable '{}' is not set", var)
            }),
            Credential::OauthToken { .. } => {
                Err(anyhow::anyhow!("credential is an OAuth token, not an API key"))
            }
        }
    }
}

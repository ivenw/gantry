use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::provider::ProviderAlias;

/// The full set of credentials, keyed by provider alias.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct CredentialsCatalog(pub HashMap<ProviderAlias, StoredCredential>);

impl CredentialsCatalog {
    /// Resolves and returns the credential for the given provider alias.
    ///
    /// Returns `None` if no credential is configured for the alias, or an error
    /// if resolution fails (e.g. a referenced environment variable is unset).
    pub fn get(&self, alias: &ProviderAlias) -> anyhow::Result<Option<Credential>> {
        self.0.get(alias).map(StoredCredential::resolve).transpose()
    }
}

/// A credential as stored on disk; may reference an environment variable.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StoredCredential {
    /// A literal API key value stored directly in the credentials file.
    ApiKey { value: String },
    /// An API key read from the named environment variable at the point of use.
    ApiKeyEnv { var: String },
    /// An OAuth token set managed by the application.
    OauthToken {
        access_token: String,
        refresh_token: String,
        expires_at: String,
    },
}

impl StoredCredential {
    /// Resolves the credential into a [`Credential`], expanding `ApiKeyEnv` by
    /// reading the environment variable. Returns an error if the variable is unset.
    pub fn resolve(&self) -> anyhow::Result<Credential> {
        match self {
            StoredCredential::ApiKey { value } => Ok(Credential::ApiKey(ApiKeyCredential {
                value: value.clone(),
            })),
            StoredCredential::ApiKeyEnv { var } => {
                let value = std::env::var(var)
                    .map_err(|_| anyhow::anyhow!("environment variable '{}' is not set", var))?;
                Ok(Credential::ApiKey(ApiKeyCredential { value }))
            }
            StoredCredential::OauthToken {
                access_token,
                refresh_token,
                expires_at,
            } => Ok(Credential::OauthToken(OauthCredential {
                access_token: access_token.clone(),
                refresh_token: refresh_token.clone(),
                expires_at: expires_at.clone(),
            })),
        }
    }
}

/// A fully resolved credential with no references to external state.
#[derive(Debug, Clone)]
pub enum Credential {
    /// A literal API key.
    ApiKey(ApiKeyCredential),
    /// An OAuth token set managed by the application.
    OauthToken(OauthCredential),
}

/// A resolved API key credential.
#[derive(Debug, Clone)]
pub struct ApiKeyCredential {
    pub value: String,
}

/// A resolved OAuth token credential.
#[derive(Debug, Clone)]
pub struct OauthCredential {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: String,
}

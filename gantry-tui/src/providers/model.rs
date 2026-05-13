use gantry_core::{ProviderAlias, ProviderConfig, StoredCredential};

/// Top-level state for the providers overlay.
pub struct ProvidersView {
    pub providers: Vec<ProviderConfig>,
    pub sub: ProvidersSubView,
}

/// Which sub-screen of the providers overlay is active.
pub enum ProvidersSubView {
    /// The list of configured providers with add/remove actions.
    List { selected_idx: usize },
    /// Picking which provider type to add.
    TypePicker { selected_idx: usize },
    /// Picking the authentication method for GitHub Copilot.
    CopilotAuthPicker { selected_idx: usize },
    /// Filling in the fields for a new provider.
    Wizard(ProviderWizard),
}

/// A field in the provider wizard — a label, an editable string value, and whether it is required.
pub struct WizardField {
    pub label: &'static str,
    pub value: String,
    pub required: bool,
}

impl WizardField {
    /// Creates a required wizard field with the given label.
    pub fn required(label: &'static str) -> Self {
        Self {
            label,
            value: String::new(),
            required: true,
        }
    }

    /// Creates an optional wizard field with the given label.
    pub fn optional(label: &'static str) -> Self {
        Self {
            label,
            value: String::new(),
            required: false,
        }
    }
}

/// Authentication method chosen for GitHub Copilot.
///
/// # How Copilot auth works
///
/// Accessing `api.githubcopilot.com` requires a short-lived Copilot token obtained by
/// exchanging a GitHub OAuth access token against `api.github.com/copilot_internal/v2/token`.
/// Not all OAuth tokens satisfy that endpoint — it requires a token issued by the GitHub
/// Copilot VS Code extension's registered OAuth app (client ID `Iv1.b507a08c87ecfe98`).
/// Tokens from `gh auth login` (even with the `copilot` scope) are issued under the gh CLI's
/// client ID and are rejected with 404.
///
/// The correct flow (used by tools like opencode and pi) is the OAuth device code flow:
/// 1. POST `github.com/login/device/code` with `client_id=Iv1.b507a08c87ecfe98&scope=read:user`
/// 2. Show the user the returned `user_code` and `verification_uri`
/// 3. Poll `github.com/login/oauth/access_token` until the user completes auth in a browser
/// 4. Store the resulting access token — it passes the `copilot_internal/v2/token` exchange
///
/// The `GhCli` variant currently calls `gh auth token`, which will only work if the user has
/// re-authed with the device code flow above. This variant should be replaced with an in-app
/// device code flow (TODO).
///
/// The `ApiKey` variant is reserved for Copilot API keys (Business/Enterprise plans only);
/// individual plan subscribers cannot generate these from `github.com/settings/copilot`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CopilotAuthKind {
    /// Obtain an OAuth token from the GitHub CLI (`gh auth token`).
    GhCli,
    /// Supply a GitHub Copilot API key directly (Business/Enterprise plans only).
    ApiKey,
}

impl CopilotAuthKind {
    /// Human-readable label shown in the auth picker.
    pub const fn label(self) -> &'static str {
        match self {
            Self::GhCli => "GitHub CLI (OAuth)",
            Self::ApiKey => "API Key",
        }
    }

    pub const ALL: &'static [CopilotAuthKind] = &[Self::GhCli, Self::ApiKey];
}

/// The provider type being configured in the wizard.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WizardProviderKind {
    Ollama,
    Copilot,
    OpenAiCompletions,
    OpenAiResponses,
    Cortecs,
}

impl WizardProviderKind {
    /// Human-readable label shown in the type picker.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ollama => "Ollama",
            Self::Copilot => "GitHub Copilot",
            Self::OpenAiCompletions => "OpenAI Completions",
            Self::OpenAiResponses => "OpenAI Responses",
            Self::Cortecs => "Cortecs",
        }
    }

    pub const ALL: &'static [WizardProviderKind] = &[
        Self::Ollama,
        Self::Copilot,
        Self::OpenAiCompletions,
        Self::OpenAiResponses,
        Self::Cortecs,
    ];
}

/// State for filling in fields for a new provider.
pub struct ProviderWizard {
    pub kind: WizardProviderKind,
    /// Authentication method for Copilot; `None` for non-Copilot providers.
    pub copilot_auth: Option<CopilotAuthKind>,
    /// Editable fields, followed by a virtual Confirm entry (always last).
    pub fields: Vec<WizardField>,
    /// Index of the currently focused row (field or the confirm entry).
    pub focused_idx: usize,
    /// Cursor byte-offset within the focused field's value string.
    pub cursor: usize,
    /// Error message shown when the user attempts to confirm with invalid state.
    pub error: Option<String>,
}

impl ProviderWizard {
    /// Builds a wizard pre-populated with the correct fields for `kind`.
    ///
    /// For `WizardProviderKind::Copilot`, `copilot_auth` selects the credential flow.
    pub fn new(kind: WizardProviderKind, copilot_auth: Option<CopilotAuthKind>) -> Self {
        let fields = match kind {
            WizardProviderKind::Ollama => vec![
                WizardField::required("Alias"),
                WizardField::optional("Base URL"),
            ],
            WizardProviderKind::Copilot => match copilot_auth {
                Some(CopilotAuthKind::ApiKey) => vec![
                    WizardField::required("Alias"),
                    WizardField::required("API Key"),
                ],
                _ => vec![
                    WizardField::required("Alias"),
                    // Token is obtained from `gh auth token` on confirm.
                ],
            },
            WizardProviderKind::OpenAiCompletions => vec![
                WizardField::required("Alias"),
                WizardField::required("Base URL"),
                WizardField::required("API Key"),
            ],
            WizardProviderKind::OpenAiResponses => vec![
                WizardField::required("Alias"),
                WizardField::required("Base URL"),
                WizardField::required("API Key"),
            ],
            WizardProviderKind::Cortecs => vec![
                WizardField::required("Alias"),
                WizardField::required("API Key"),
            ],
        };
        Self {
            kind,
            copilot_auth,
            fields,
            focused_idx: 0,
            cursor: 0,
            error: None,
        }
    }

    /// Returns the number of rows in the wizard (fields + confirm).
    pub fn row_count(&self) -> usize {
        self.fields.len() + 1
    }

    /// Returns true if the focused row is the Confirm entry.
    pub fn is_on_confirm(&self) -> bool {
        self.focused_idx == self.fields.len()
    }

    /// Validates fields and builds the `ProviderConfig` and optional `StoredCredential`.
    ///
    /// Returns an error string if any required field is empty.
    pub fn build(&self) -> Result<(ProviderConfig, Option<StoredCredential>), String> {
        for f in &self.fields {
            if f.required && f.value.trim().is_empty() {
                return Err(format!("'{}' is required", f.label));
            }
        }

        let alias = ProviderAlias::new(self.fields[0].value.trim());

        match self.kind {
            WizardProviderKind::Ollama => {
                let base_url = self.fields[1].value.trim();
                let config = ProviderConfig::Ollama(gantry_core::OllamaProviderConfig {
                    alias,
                    base_url: if base_url.is_empty() {
                        None
                    } else {
                        Some(base_url.to_string())
                    },
                    context_length: None,
                });
                Ok((config, None))
            }
            WizardProviderKind::Copilot => {
                let config = ProviderConfig::Copilot(gantry_core::CopilotProviderConfig { alias });
                let credential = match self.copilot_auth {
                    Some(CopilotAuthKind::ApiKey) => {
                        let api_key = self.fields[1].value.trim().to_string();
                        StoredCredential::ApiKey { value: api_key }
                    }
                    _ => {
                        let token = obtain_gh_token()?;
                        // TODO: implement token refresh via `gh auth token` when the access token
                        // expires, rather than storing a refresh_token or expires_at.
                        StoredCredential::OauthToken {
                            access_token: token,
                            refresh_token: String::new(),
                            expires_at: String::new(),
                        }
                    }
                };
                Ok((config, Some(credential)))
            }
            WizardProviderKind::OpenAiCompletions => {
                let base_url = self.fields[1].value.trim().to_string();
                let api_key = self.fields[2].value.trim().to_string();
                let config = ProviderConfig::OpenAiCompletions(
                    gantry_core::OpenAiCompletionsProviderConfig { alias, base_url },
                );
                Ok((config, Some(StoredCredential::ApiKey { value: api_key })))
            }
            WizardProviderKind::OpenAiResponses => {
                let base_url = self.fields[1].value.trim().to_string();
                let api_key = self.fields[2].value.trim().to_string();
                let config =
                    ProviderConfig::OpenAiResponses(gantry_core::OpenAiResponsesProviderConfig {
                        alias,
                        base_url,
                    });
                Ok((config, Some(StoredCredential::ApiKey { value: api_key })))
            }
            WizardProviderKind::Cortecs => {
                let api_key = self.fields[1].value.trim().to_string();
                let config = ProviderConfig::Cortecs(gantry_core::CortecsProviderConfig { alias });
                Ok((config, Some(StoredCredential::ApiKey { value: api_key })))
            }
        }
    }
}

/// Invokes `gh auth token` and returns the trimmed token string.
///
/// Returns an error string if `gh` is not installed, not authenticated, or the command fails.
fn obtain_gh_token() -> Result<String, String> {
    let output = std::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "GitHub CLI (`gh`) not found — install it and run `gh auth login`".to_string()
            } else {
                format!("failed to run `gh auth token`: {e}")
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "`gh auth token` failed — run `gh auth login` first\n{}",
            stderr.trim()
        ));
    }

    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() {
        return Err("`gh auth token` returned an empty token — run `gh auth login`".to_string());
    }

    Ok(token)
}

use thiserror::Error;

use super::messages;

/// Errors that can occur at the rig tool boundary regardless of which tool is called.
#[derive(Debug, Error)]
pub enum BoundaryError {
    #[error("{}", join_error(.0))]
    Join(#[from] tokio::task::JoinError),
}

fn join_error(err: &tokio::task::JoinError) -> String {
    messages::join_error(err)
}

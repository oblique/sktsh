use thiserror::Error;

#[derive(Debug, Error)]
pub enum HandleClientError {
    #[error("Failed to forward client data to PTY")]
    ClientToPtyFailed,

    #[error("Failed to forward PTY data to client")]
    PtyToClientFailed,
}

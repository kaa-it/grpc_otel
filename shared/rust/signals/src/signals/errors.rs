use thiserror::Error;

#[derive(Error, Debug)]
pub enum SignalsError {
    #[error("Signals already registered")]
    AlreadyRegistered,

    #[error("Signals already unregistered")]
    AlreadyUnregistered,

    #[cfg(target_os = "windows")]
    #[error(transparent)]
    InternalError(#[from] ctrlc::Error),
}

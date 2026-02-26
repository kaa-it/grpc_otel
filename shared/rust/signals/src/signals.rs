#[cfg(not(target_os = "windows"))]
#[path = "./signals/linux.rs"]
mod linux;

#[cfg(target_os = "windows")]
#[path = "./signals/windows.rs"]
mod windows;

pub mod errors;

#[cfg(not(target_os = "windows"))]
pub use linux::*;

#[cfg(target_os = "windows")]
pub use windows::*;

#[cfg(test)]
mod tests {
    use std::time::Duration;
    use tokio::sync::watch;
    use super::*;

    #[tokio::test]
    async fn signals_can_register_and_unregister() {
        let (sender, _) = watch::channel("work");

        _ = signals_register(&sender).await.unwrap();

        tokio::time::sleep(Duration::from_secs(2)).await;

        signals_unregister().await.unwrap();
    }
}

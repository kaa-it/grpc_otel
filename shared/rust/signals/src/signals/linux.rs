use super::errors::SignalsError;
use futures_util::StreamExt;
use signal_hook::consts::signal::*;
use signal_hook_tokio::{Handle, Signals};
use tokio::sync::Mutex;
use tokio::{sync::watch::Sender, task::JoinHandle};
use once_cell::sync::Lazy;

static SIGNALS_INFO: Lazy<Mutex<Option<SignalsInfo>>> = Lazy::new(|| { Mutex::new(None) });

struct SignalsInfo {
    handle: Handle,
    signals_task: JoinHandle<()>,
}

pub async fn signals_register(sender: &Sender<&'static str>) -> Result<(), SignalsError> {
    let mut signals_info = SIGNALS_INFO.lock().await;

    if signals_info.is_some() {
        return Err(SignalsError::AlreadyRegistered);
    }

    let signals = Signals::new([SIGINT, SIGTERM]).unwrap();
    let handle = signals.handle();

    let s_sender = sender.clone();
    let signals_task = tokio::spawn(handle_signals(signals, s_sender));

    *signals_info = Some(SignalsInfo {
        handle,
        signals_task,
    });

    Ok(())
}

pub async fn signals_unregister() -> Result<(), SignalsError> {
    let mut signals_info = SIGNALS_INFO.lock().await;

    if signals_info.is_none() {
        return Err(SignalsError::AlreadyUnregistered);
    }

    let si = signals_info.take().unwrap();

    si.handle.close();
    si.signals_task.await.unwrap();

    Ok(())
}

async fn handle_signals(mut signals: Signals, stop_channel: Sender<&'static str>) {
    while let Some(signal) = signals.next().await {
        match signal {
            SIGTERM | SIGINT => {
                let _ = stop_channel.send("close");
            }
            _ => unreachable!(),
        }
    }
}

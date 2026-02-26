use std::time::Duration;

use anyhow::Result;
use tokio::sync::watch::Sender;
use windows_service::{
    service::{ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType},
    service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle}, Error
};

use super::errors::SignalsError;

pub async fn signals_register(sender: &Sender<&'static str>) -> Result<(), SignalsError> {
    println!("set handler");

    let c_sender = sender.clone();
    ctrlc::set_handler( move || {
        println!("Send close at Ctrl-C");
        _ = c_sender.send("close");
    })?;

    Ok(())
}

pub async fn signals_unregister() -> Result<(), SignalsError> {
    Ok(())
}

pub async fn service_register(sender: &Sender<&'static str>) -> Result<ServiceStatusHandle, Error> {
    println!("set service handler");

    let service_sender = sender.clone();
    let status_handler = service_control_handler::register(
        "device controlle",
        move |control_event| match control_event {
            ServiceControl::Stop | ServiceControl::Shutdown | ServiceControl::Interrogate => {
                _ = service_sender.send("close");
                ServiceControlHandlerResult::NoError
            },
            _ => ServiceControlHandlerResult::NotImplemented
        },
    )?;

    status_handler.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::from_secs(10),
        process_id: None,
    })?;

    Ok(status_handler)
}

pub async fn service_unregister(status_handler: ServiceStatusHandle) -> Result<(), Error> {    
    status_handler.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::from_secs(10),
        process_id: None,
    })?;

    Ok(())
}
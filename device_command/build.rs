use std::{env, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .file_descriptor_set_path(out_dir.join("command_service_descriptor.bin"))
        .compile_protos(
            &["../proto/device_command/v1/device_command.proto"],
            &["../proto/device_command/v1"],
        )?;

    println!("cargo:rerun-if-changed=../proto/device_command/v1/device_command.proto");

    Ok(())
}

use std::{iter::once, net::TcpListener, path::Path, process::Command, thread::sleep, time::Duration};

use clap::Parser;
use dll_syringe::{process::{OwnedProcess, Process}, Syringe};
use tiny_pinch::{Arguments, GLADE_DIR, GLADE_PATH, GLADE_PDB_PATH};
use tracing::{info, Level};

fn main() -> anyhow::Result<()> {
    dotenvy::dotenv()?;

    tracing_subscriber::fmt().with_max_level(Level::DEBUG).init();

    let args = Arguments::parse();

    info!("Glade path: {:?}", *GLADE_PATH);

    let mut tiny_glade_command = Command::new(&*GLADE_PATH);

    tiny_glade_command.current_dir(&*GLADE_DIR);

    let arguments = match args.additional_arguments {
        Some(arguments) => {
            once(args.dll_path.to_string_lossy().to_string())
                .chain(arguments.into_iter())
                .collect()
        },
        None => {
            vec![args.dll_path.to_string_lossy().to_string()]
        },
    };

    tiny_glade_command.env("TINY_PINCH_ARGUMENTS", shell_words::join(arguments));
    tiny_glade_command.env("TINY_PINCH_PDB", &*GLADE_PDB_PATH);
    
    let tiny_glade_process = tiny_glade_command.spawn()?;

    info!("Launched Tiny Glade process: {}", tiny_glade_process.id());

    let target_process = OwnedProcess::from(tiny_glade_process);
    let syringe = Syringe::for_process(target_process);
    info!("Created syringe for Tiny Glade");

    sleep(Duration::from_secs_f32(args.delay));

    let result = inject(&syringe, &args.dll_path);

    syringe.process().kill()?;

    result
}

fn inject(syringe: &Syringe, dll_path: impl AsRef<Path>) -> anyhow::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8996")?;

    let _injected_payload = syringe.inject(&dll_path)?;
    info!("Injected successfully");

    let (mut stream, address) = listener.accept()?;
    info!("Connected to process at: {address}");

    let mut stdout = std::io::stdout();
    std::io::copy(&mut stream, &mut stdout)?;

    Ok(())
}

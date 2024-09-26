pub mod glade_path;

use std::{env, path::PathBuf};

use clap::Parser;
use glade_path::get_glade_dir;
use lazy_static::lazy_static;

lazy_static! {
    pub static ref GLADE_DIR: PathBuf = get_glade_dir();
    pub static ref GLADE_PATH: PathBuf = GLADE_DIR.join(env::var("TINY_PINCH_GLADE_EXE").unwrap_or_else(|_| String::from("tiny-glade.exe")));
    pub static ref GLADE_PDB_PATH: PathBuf = GLADE_DIR.join(env::var("TINY_PINCH_GLADE_PDB").unwrap_or_else(|_| String::from("tiny_glade.pdb")));
}

#[derive(Debug, Parser)]
pub struct Arguments {
    #[clap(long, short, default_value = "0.0")]
    pub delay: f32,
    pub dll_path: PathBuf,
    #[arg(last(true))]
    pub additional_arguments: Option<Vec<String>>,
}

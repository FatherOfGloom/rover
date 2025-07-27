use std::{env, io::stdout};

use crate::rover::DirScraper;

pub mod rover;

pub fn run() -> Result<(), String> {
    println!("Ur mom.");

    let path = env::current_dir().map_err(|e| format!("Error: {}", e.to_string()))?;

    let mut stdout = stdout().lock();
    let mut ds = DirScraper::init(path);

    ds.run(&mut stdout)
        .map_err(|e| format!("Error: {}", e.to_string()))?;

    Ok(())
}

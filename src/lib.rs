use std::env;

use crate::rover::DirScraper;

pub mod rover;

pub fn run() -> Result<(), String> {
    println!("Ur mom.");

    let path = env::current_dir().map_err(|e| format!("Error: {}", e.to_string()))?;

    // let mut r = Rover::new(&path).unwrap();
    let mut ds = DirScraper::init(path);

    while !ds.should_exit() {
        ds.render();
        // ds.update().unwrap();
    }

    Ok(())
}
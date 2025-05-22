use crossterm::terminal;
use rover::Rover;

use std::env;

pub mod rover;

pub fn run() -> Result<(), String> {
    println!("Ur mom.");
    
    terminal::enable_raw_mode().map_err(|e| format!("Couldn't enable raw mode: {}", e.to_string()))?;
    let path = env::current_dir().map_err(|e| format!("Error: {}", e.to_string()))?;

    let mut r = Rover::new(&path).unwrap();

    while !r.should_exit() {
        r.draw_console();
        r.update().unwrap();
    }

    terminal::disable_raw_mode().map_err(|e| format!("Couldn't disable raw mode: {}", e.to_string()))?;

    Ok(())
}
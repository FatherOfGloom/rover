use rover::Rover;

use std::{env, io::stdout};

pub mod rover;

pub fn run() -> Result<(), String> {
    println!("Ur mom.");

    let path = env::current_dir().map_err(|e| format!("Error: {}", e.to_string()))?;

    let mut r = Rover::new(&path).unwrap();

    while !r.should_exit() {
        Rover::flush_console(&mut stdout()).unwrap();
        r.draw_console().unwrap();
        r.update().unwrap();
    }

    Ok(())
}

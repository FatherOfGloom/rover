use rover::Rover;

use std::env;

pub mod rover;

pub fn run() -> Result<(), String> {
    println!("Ur mom.");

    let path = env::current_dir().map_err(|e| format!("Error: {}", e.to_string()))?;

    let mut r = Rover::new(&path).unwrap();

    while !r.should_exit() {
        r.render().unwrap();
        r.update().unwrap();
    }

    Ok(())
}

use rover::Rover;

use std::env;

pub mod rover;

pub fn run() {
    println!("Ur mom.");

    Rover::read_dir(&env::current_dir().unwrap()).unwrap();
}
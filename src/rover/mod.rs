use std::{path::Path};
use std::fs::{self, DirEntry};

use crossterm::terminal;

#[derive(Default)]
pub struct Rover {
    entries: Option<Vec<DirEntry>>,
    should_exit: bool,
    pivot: usize,
}

impl Rover {
    pub fn new(path: &Path) -> Result<Self, String> {
        Ok(Rover {
            entries: Some(Self::read_dir(path)?),
            ..Default::default()
        })
    }

    pub fn update(&mut self) -> std::io::Result<()> {
        self.should_exit = Self::read_console_input()?; 
        Ok(())
    }

    pub fn draw_console(&self) {
        let entries = self.entries.as_ref().expect("draw_console() called before initializing entry list");

        for (i, e) in entries.iter().enumerate() {
            println!("\t{} {}", if i == self.pivot { ">" } else { " " }, e.path().display());
        }
    }

    fn read_console_input() -> std::io::Result<bool> {
        Ok(true)
    }

    pub fn should_exit(&self) -> bool {
        self.should_exit
    }

    fn read_dir(path: &Path) -> Result<Vec<DirEntry>, String> { 
        if !path.exists() {
            return Err(format!("Given path doesn't exist: '{}'", path.display()))
        }

        if !path.is_dir() {
            return Err(format!("Given path is not a directory: '{}'", path.display()));
        }

        let res = fs::read_dir(path).unwrap().map(|e| e.unwrap()).collect();

        Ok(res)
    }
}
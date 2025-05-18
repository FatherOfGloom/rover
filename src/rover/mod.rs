use std::{fmt::format, path::Path, thread::panicking};

use std::fs::{self, DirEntry};

use std::io;

pub struct Rover {
    dir_entries: Option<Vec<DirEntry>>
}

impl Rover {
    pub fn new() -> Self {
        Rover {
            dir_entries: None, 
        }
    }

    pub fn read_dir(path: &Path) -> Result<(), String> { 
        if !path.exists() {
            return Err(format!("Given path doesn't exist: '{}'", path.display()))
        }

        if !path.is_dir() {
            return Err(format!("Given path is not a directory: '{}'", path.display()));
        }

        fs::read_dir(path).unwrap().for_each(|entry| println!("{}", entry.unwrap().path().display()));

        Ok(())
    }
}
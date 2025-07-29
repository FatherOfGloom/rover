use std::{
    env,
    io::{self, stdout},
};

use crate::rover::DirScraper;

pub mod rover;

pub fn run() -> io::Result<()> {
    println!("Ur mom.");

    let path = env::current_dir()?;

    let mut stdout = stdout().lock();
    let mut ds = DirScraper::init(path)?;

    ds.run(&mut stdout)?;

    Ok(())
}

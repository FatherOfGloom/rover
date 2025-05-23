use std::fs::{self, DirEntry};
use std::io::{self, Stdout, Write, stdout};
use std::ops::Add;
use std::path::Path;
use std::thread;
use std::time::Duration;

use crossterm::cursor::MoveTo;
use crossterm::event::{Event, KeyCode, KeyModifiers, read};
use crossterm::terminal::{Clear, EnterAlternateScreen};
use crossterm::{QueueableCommand, terminal};

#[derive(Default)]
pub struct Rover {
    entries: Option<Vec<DirEntry>>,
    should_exit: bool,
    pivot: usize,
}

impl Rover {
    pub fn new(path: &Path) -> Result<Self, String> {
        terminal::enable_raw_mode()
            .map_err(|e| format!("Couldn't enable raw mode: {}", e.to_string()))?;

        Self::flush_console(&mut stdout()).unwrap();

        Ok(Rover {
            entries: Some(Self::read_dir(path)?),
            ..Default::default()
        })
    }

    pub fn update(&mut self) -> io::Result<()> {
        self.read_console_input()?;
        Ok(())
    }

    pub fn flush_console(stdout: &mut Stdout) -> io::Result<()> {
        stdout.queue(Clear(terminal::ClearType::All)).unwrap();
        stdout.queue(MoveTo(0, 0))?;
        stdout.flush()?;
        Ok(())
    }

    pub fn draw_console(&self) -> io::Result<()> {
        let entries = self
            .entries
            .as_ref()
            .expect("draw_console() called before initializing entry list");


        for (i, e) in entries.iter().enumerate() {
            println!(
                "\t{} {}",
                if i == self.pivot { ">" } else { " " },
                e.path().display()
            );
        }

        thread::sleep(Duration::from_millis(50));

        Ok(())
    }

    fn read_console_input(&mut self) -> std::io::Result<()> {
        match read().unwrap() {
            Event::Key(event) => match event.code {
                KeyCode::Char(c) => {
                    if event.modifiers.contains(KeyModifiers::CONTROL) {
                        if c == 'c' {
                            self.should_exit = true;
                        }
                    } else {
                        match c.to_lowercase().next().unwrap() {
                            'j' => self.shift(RoverDirection::DOWN),
                            'k' => self.shift(RoverDirection::UP),
                            _ => {}
                        }
                    }
                }
                KeyCode::Up => self.shift(RoverDirection::UP),
                KeyCode::Down => self.shift(RoverDirection::DOWN),
                _ => {}
            },
            _ => {}
        }
        Ok(())
    }

    fn shift(&mut self, d: RoverDirection) {
        match d {
            RoverDirection::UP => {
                if self.pivot as i64 - 1 < 0 {
                    self.pivot = self.len()
                } else {
                    self.pivot -= 1;
                }
            }
            RoverDirection::DOWN => {
                self.pivot += 1;
                self.pivot %= self.len()
            }
        };
    }

    pub fn len(&self) -> usize {
        self.entries.as_ref().unwrap().len()
    }

    pub fn should_exit(&self) -> bool {
        self.should_exit
    }

    fn read_dir(path: &Path) -> Result<Vec<DirEntry>, String> {
        if !path.exists() {
            return Err(format!("Given path doesn't exist: '{}'", path.display()));
        }

        if !path.is_dir() {
            return Err(format!(
                "Given path is not a directory: '{}'",
                path.display()
            ));
        }

        let res = fs::read_dir(path).unwrap().map(|e| e.unwrap()).collect();

        Ok(res)
    }
}

enum RoverDirection {
    UP,
    DOWN,
}

impl Drop for Rover {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode()
            .map_err(|e| eprintln!("Error: Couldn't disable raw mode: {}", e.to_string()));
    }
}

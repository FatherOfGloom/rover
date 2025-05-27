use core::fmt;
use std::fs::{self, DirEntry};
use std::io::{self, Stdout, Write, stdout};
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{thread, usize};

use crossterm::cursor::MoveTo;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, read};
use crossterm::terminal::Clear;
use crossterm::{QueueableCommand, terminal};

#[derive(Default, Clone, Copy)]
pub struct Rect {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

impl Rect {
    pub fn new(x: usize, y: usize, w: usize, h: usize) -> Self {
        Rect { x, y, w, h }
    }

    pub fn resize(&mut self, w: usize, h: usize) {
        self.w = w;
        self.h = h;
    }
}

#[derive(Default)]
pub struct Rover {
    current_path: Option<PathBuf>,
    entries: Option<Vec<DirEntry>>,
    should_exit: bool,
    pivot: usize,
    mode: RoverMode,
    console_screen: Rect,
}

impl Rover {
    pub fn new(path: &Path) -> Result<Self, String> {

        Self::flush_console(&mut stdout()).unwrap();
        let (w, h) = terminal::size().unwrap();

        Ok(Rover {
            entries: Some(Self::read_dir(path)?),
            current_path: Some(path.to_path_buf()),
            console_screen: Rect::new(0, 0, w as usize, h as usize),
            ..Default::default()
        })
    }

    pub fn update(&mut self) -> Result<(), String> {
        self.read_console_input()?;
        Ok(())
    }

    pub fn flush_console(stdout: &mut Stdout) -> io::Result<()> {
        stdout.queue(Clear(terminal::ClearType::All)).unwrap();
        stdout.queue(MoveTo(0, 0))?;
        stdout.flush()?;
        Ok(())
    }

    fn current_path_ref(&self) -> &Path {
        self.current_path.as_ref().unwrap()
    }

    pub fn draw_entries(&self) -> io::Result<()> {
        let entries = self
            .entries
            .as_ref()
            .expect("draw_console() called before initializing entry list");

        let mut stdout = stdout();

        stdout.queue(Clear(terminal::ClearType::All))?;
        stdout.queue(MoveTo(0, 0))?;

        stdout.write(format!("DIR: {}", self.current_path_ref().display()).as_bytes())?;
        stdout.queue(MoveTo(0, 1))?;

        let mode_label = self.mode.to_string();

        stdout.queue(MoveTo(
            (self.console_screen.w - mode_label.len()) as u16, 0
        ))?;

        stdout.write(mode_label.as_bytes())?;

        for (i, e) in entries.iter().enumerate() {
            let prefix = if i == self.pivot { ">" } else { " " };
            let dir_entry_path = format!("{} {}", prefix, e.file_name().to_str().unwrap());
            stdout.queue(MoveTo(0, i as u16 + 3))?;
            stdout.write(dir_entry_path.as_bytes())?;
        }

        stdout.flush()?;

        thread::sleep(Duration::from_millis(33));

        Ok(())
    }

    fn read_console_input(&mut self) -> Result<(), String> {
        match read().unwrap() {
            Event::Key(event) => {
                if event.kind != KeyEventKind::Press {
                    return Ok(());
                }

                match event.code {
                    KeyCode::Char(c) => {
                        if event.modifiers.contains(KeyModifiers::CONTROL) {
                            match c {
                                'q' => self.should_exit = true,
                                'c' => self.mode = RoverMode::COMMAND,
                                'f' => self.mode = RoverMode::FLOW,
                                'k' => self.execute_entry()?,
                                _ => {}
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
                    KeyCode::Enter => self.execute_entry()?,
                    KeyCode::Esc => self.to_parent_entry()?,
                    _ => {}
                }
            }
            Event::Resize(new_w, new_h) => {
                self.console_screen.resize(new_w as usize, new_h as usize);
            }
            _ => {}
        }
        Ok(())
    }

    fn entries_ref(&self) -> &Vec<DirEntry> {
        self.entries.as_ref().unwrap()
    }

    fn execute_entry(&mut self) -> Result<(), String> {
        let selected_path = self.entries_ref().get(self.pivot).unwrap().path();

        if selected_path.is_dir() {
            self.goto(&selected_path)?;
        } else {
            opener::open(selected_path.display().to_string())
                .map_err(|e| format!("Error opening the file '{}': {}", selected_path.display(), e))?;
        }

        Ok(())
    }

    fn to_parent_entry(&mut self) -> Result<(), String> {
        // Disgusting borrow checker doesn't let me borrow self.current_path.ancestors() directly
        // Bc the iterator borrows self immutably so we have to wrap it into Option and take() it i guess, dont want to clone for no reason
        let current_path = self.current_path.take().unwrap();

        let mut cur_ancestors = current_path.ancestors();
        // advance the iterator once since it returns current path
        cur_ancestors.next().ok_or(format!("Current path '{}' is invalid.", current_path.display()))?;

        let Some(parent_entry) = cur_ancestors.next() else {
            self.current_path = Some(current_path);
            return Err("Cannot move back from root folder.".to_string());
        };

        self.goto(parent_entry).map_err(|e| { self.current_path = Some(current_path); e })?;

        Ok(())
    }

    fn goto(&mut self, selected_path: &Path) -> Result<(), String> {
        if !selected_path.exists() {
            return Err(format!("Chosen folder '{}' doesn't exist.", selected_path.display()));
        }

        self.entries = Some(Self::read_dir(&selected_path).unwrap());
        self.current_path = Some(selected_path.to_path_buf());
        self.pivot = 0;

        Ok(())
    }

    fn shift(&mut self, d: RoverDirection) {
        match d {
            RoverDirection::UP => {
                if self.pivot as i64 - 1 < 0 {
                    self.pivot = self.len() - 1;
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
            return Err(format!("Given path doesn't exist: '{}'.", path.display()));
        }

        if !path.is_dir() {
            return Err(format!(
                "Given path is not a directory: '{}'.",
                path.display()
            ));
        }

        let res = fs::read_dir(path).unwrap().map(|e| e.unwrap()).collect();

        Ok(res)
    }
}

#[derive(Clone, Copy, Debug)]
enum RoverMode {
    FLOW,
    COMMAND,
}

impl fmt::Display for RoverMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl Default for RoverMode {
    fn default() -> Self {
        RoverMode::FLOW
    }
}

enum RoverDirection {
    UP,
    DOWN,
}
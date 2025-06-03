use core::fmt;
use std::fs::{self, DirEntry};
use std::io::{self, Stdout, Write, stdout};
use std::path::{Path, PathBuf};
use std::{usize};

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
    flow_direction: RoverDirection,
    undo_path: Option<Vec<usize>>
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

    fn draw_entries(&self, stdout: &mut impl Write, entries: &[DirEntry], rect: Rect) -> io::Result<()> {
        let entries: &[DirEntry] = match self.flow_direction {
            RoverDirection::Down | RoverDirection::None  | RoverDirection::Up => {
                if self.pivot + 1 > rect.h {
                    &entries[self.pivot - rect.h + 1..self.pivot + 1]
                } else {
                    entries
                }
            }, 
        };

        for (i, e) in entries.iter().enumerate() {
            if i + rect.y + 1 > self.console_screen.h {
                break;
            }

            let prefix = if i == std::cmp::min(rect.h - 1, self.pivot) { ">" } else { " " };
            let dir_entry_path = format!("{} {}", prefix, e.file_name().to_str().unwrap());
            stdout.queue(MoveTo(rect.x as u16, i as u16 + rect.y as u16))?;
            stdout.write(dir_entry_path.as_bytes())?;
        }
        Ok(())
    }

    pub fn render(&self) -> io::Result<()> {
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

        self.draw_entries(&mut stdout, &entries, Rect { x: 1, y: 2, w: self.console_screen.w - 1, h: self.console_screen.h - 2})?;

        stdout.flush()?;

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
                                'c' => self.mode = RoverMode::Command,
                                'f' => self.mode = RoverMode::Flow,
                                'k' => self.execute_entry()?,
                                _ => {}
                            }
                        } else {
                            match c.to_lowercase().next().unwrap() {
                                'j' => self.shift(RoverDirection::Down),
                                'k' => self.shift(RoverDirection::Up),
                                _ => {}
                            }
                        }
                    }
                    KeyCode::Up => self.shift(RoverDirection::Up),
                    KeyCode::Down => self.shift(RoverDirection::Down),
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
            let prev_pivot = self.pivot;
            self.goto(&selected_path)?;
            self.push_undo_pivot(prev_pivot);
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
        self.pivot = self.pop_undo_pivot();

        Ok(())
    }

    fn push_undo_pivot(&mut self, idx: usize) {
        match self.undo_path.as_mut() {
            Some(v) => v.push(idx),
            None => self.undo_path = Some(vec![idx]),
        }
    }

    fn pop_undo_pivot(&mut self) -> usize {
        match self.undo_path.as_mut() {
            Some(v) => v.pop().unwrap_or(0),
            _ => 0,
        }
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
            RoverDirection::Up => {
                if self.pivot as i64 - 1 < 0 {
                    self.pivot = self.len() - 1;
                } else {
                    self.pivot -= 1;
                }
            }
            RoverDirection::Down => {
                self.pivot += 1;
                self.pivot %= self.len()
            }
            _ => panic!("Cannot shift() to uncertain direction.")
        };

        self.flow_direction = d;
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

#[derive(Default, Debug)]
enum RoverMode {
    #[default]
    Flow,
    Command,
}

impl fmt::Display for RoverMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[derive(Default)]
enum RoverDirection {
    #[default]
    None,
    Up,
    Down,
}
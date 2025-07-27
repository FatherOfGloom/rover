use core::fmt;
use std::fs::{self, DirEntry};
use std::io::{self, Stdout, Write, stdout};
use std::path::{Path, PathBuf};
use std::usize;

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, read};
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

impl Component for DirEntry {
    fn render(&self, out: &mut impl Write) {
        out.write(self.file_name().to_str().unwrap().as_bytes());
    }
}

pub struct DirScraper {
    pub should_exit: bool,
    current_path: Option<PathBuf>,
    rover: Rover<DirEntry, ListRenderer>,
    mode: Mode,
}

impl DirScraper {
    pub fn init(path: PathBuf) -> Self {
        let (w, h) = terminal::size().unwrap();
        let dimens = Rect::new(0, 0, w as usize, h as usize);

        let mut rover = Rover::new(dimens, ListRenderer::new(dimens));

        let entries = Self::read_dir(&path).unwrap();
        rover.reset(entries);

        DirScraper {
            current_path: Some(path),
            should_exit: false,
            rover: rover,
            mode: Mode::Flow,
        }
    }

    pub fn render(&mut self) {
        self.rover.render();
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
                                'c' => self.mode = Mode::Command,
                                'f' => self.mode = Mode::Flow,
                                'k' => self.execute_entry()?,
                                _ => {}
                            }
                        } else {
                            match c.to_lowercase().next().unwrap() {
                                'j' => self.rover.shift(Direction::Down),
                                'k' => self.rover.shift(Direction::Up),
                                _ => {}
                            }
                        }
                    }
                    KeyCode::Up => self.rover.shift(Direction::Up),
                    KeyCode::Down => self.rover.shift(Direction::Down),
                    KeyCode::Enter => self.execute_entry()?,
                    KeyCode::Esc => self.to_parent_entry()?,
                    _ => {}
                }
            }
            Event::Resize(w, h) => {
                self.rover.resize(w as usize, h as usize);
            }
            _ => {}
        }
        Ok(())
    }

    fn to_parent_entry(&mut self) -> Result<(), String> {
        // Disgusting borrow checker doesn't let me borrow self.current_path.ancestors() directly
        // Bc the iterator borrows self immutably so we have to wrap it into Option and take() it i guess, dont want to clone for no reason
        let current_path = self.current_path.take().unwrap();

        let mut cur_ancestors = current_path.ancestors();
        // advance the iterator once since it returns current path
        cur_ancestors.next().ok_or(format!(
            "Current path '{}' is invalid.",
            current_path.display()
        ))?;

        let Some(parent_entry) = cur_ancestors.next() else {
            self.current_path = Some(current_path);
            return Err("Cannot move back from root folder.".to_string());
        };

        self.goto(parent_entry).map_err(|e| {
            self.current_path = Some(current_path);
            e
        })?;
        // self.pivot = self.pop_undo_pivot();

        Ok(())
    }

    // fn push_undo_pivot(&mut self, idx: usize) {
    //     match self.undo_path.as_mut() {
    //         Some(v) => v.push(idx),
    //         None => self.undo_path = Some(vec![idx]),
    //     }
    // }

    // fn pop_undo_pivot(&mut self) -> usize {
    //     match self.undo_path.as_mut() {
    //         Some(v) => v.pop().unwrap_or(0),
    //         _ => 0,
    //     }
    // }

    fn goto(&mut self, selected_path: &Path) -> Result<(), String> {
        if !selected_path.exists() {
            return Err(format!(
                "Chosen folder '{}' doesn't exist.",
                selected_path.display()
            ));
        }

        let entries = Self::read_dir(&selected_path).unwrap();
        self.current_path = Some(selected_path.to_path_buf());
        // self.pivot = 0;

        self.rover.reset(entries);

        Ok(())
    }

    fn execute_entry(&mut self) -> Result<(), String> {
        let selected_path = self.rover.get_selected_item().unwrap().path();

        if selected_path.is_dir() {
            self.goto(&selected_path)?;
            // let prev_pivot = self.pivot;
            // self.push_undo_pivot(prev_pivot);
        } else {
            opener::open(selected_path.display().to_string()).map_err(|e| {
                format!(
                    "Error opening the file '{}': {}",
                    selected_path.display(),
                    e
                )
            })?;
        }

        Ok(())
    }
}

struct Context {
    offset: usize,
    pivot: Option<usize>,
    max_visible_rows: usize,
    dimens: Rect,
}

trait Component {
    fn render(&self, w: &mut impl Write);
}

impl Component for PathBuf {
    fn render(&self, w: &mut impl Write) {
        w.write(self.file_name().unwrap().to_str().unwrap().as_bytes());
    }
}

struct ListRenderer {
    stdout: Stdout,
    bounds: Rect,
}

impl ListRenderer {
    fn new(bounds: Rect) -> Self {
        Self {
            stdout: stdout(),
            bounds,
        }
    }

    fn next_line(&mut self) {
        todo!();
    }
}

impl Renderer for ListRenderer {
    fn render<'a, I, T>(&mut self, components: I)
    where
        I: Iterator<Item = &'a T>,
        T: Component + 'a,
    {
        for c in components {
            c.render(&mut self.stdout);
            self.next_line();
        }
    }
}

trait Renderer {
    fn render<'a, I, T>(&mut self, components: I)
    where
        I: Iterator<Item = &'a T>,
        T: Component + 'a;
}

struct Rover<C, R>
where
    C: Component,
    R: Renderer,
{
    components: Option<Vec<C>>,
    ctx: Context,
    renderer: Option<R>,
}

impl<C: Component, R: Renderer> Rover<C, R> {
    pub fn new(dimens: Rect, renderer: R) -> Self {
        Rover {
            components: None,
            ctx: Context {
                offset: todo!(),
                pivot: None,
                max_visible_rows: todo!(),
                dimens,
            },
            renderer: Some(renderer),
        }
    }

    pub fn reset(&mut self, new_components: Vec<C>) {
        self.components = Some(new_components);
    }

    fn render(&mut self) {
        let offset = self.ctx.offset;
        let max_rows = self.ctx.max_visible_rows;

        let components = self
            .components
            .as_ref()
            .unwrap()
            .iter()
            .skip(offset)
            .take(max_rows);

        self.renderer.as_mut().unwrap().render(components);
    }

    fn get_selected_item(&self) -> Option<&C> {
        Some(self.components.as_ref()?.get(self.ctx.pivot?)?)
    }

    fn set_selected(&mut self, idx: usize) {
        assert!((0..self.len() - 1).contains(&idx));
        self.ctx.pivot = Some(idx);
    }

    fn resize(&mut self, w: usize, h: usize) {
        self.ctx.dimens.resize(w, h);
    }

    fn shift(&mut self, d: Direction) {
        let len = self.len();
        let pivot = self.ctx.pivot.as_mut().unwrap();

        match d {
            Direction::Up => {
                if *pivot as i64 - 1 < 0 {
                    *pivot = len - 1;
                } else {
                    *pivot -= 1;
                }
            }
            Direction::Down => {
                *pivot += 1;
                *pivot %= len;
            }
        };
    }

    pub fn len(&self) -> usize {
        self.components.as_ref().unwrap().len()
    }
}

#[derive(Default, Debug)]
enum Mode {
    #[default]
    Flow,
    Command,
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

// #[derive(Default)]
enum Direction {
    Up,
    Down,
}
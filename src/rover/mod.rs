use core::fmt;
use std::cell::RefCell;
use std::fs::{self, DirEntry};
use std::io::{self, Stdout, StdoutLock, Write, stdout};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::usize;

use crossterm::cursor::{self, MoveTo};
use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers, read,
};
use crossterm::terminal::{
    BeginSynchronizedUpdate, Clear, ClearType, DisableLineWrap, EnableLineWrap,
    EndSynchronizedUpdate, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode,
};
use crossterm::{QueueableCommand, terminal};

use crate::rover;

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
        out.write(self.file_name().to_str().unwrap().as_bytes())
            .unwrap();
    }
}

pub struct DirScraper {
    should_exit: bool,
    current_path: Option<PathBuf>,
    rover: Rover<ListEntry>,
    mode: Mode,
    terminal_dimens: Rect,
}

impl DirScraper {
    pub fn init(path: PathBuf) -> Self {
        let (w, h) = terminal::size().unwrap();
        let dimens = Rect::new(0, 0, w as usize, h as usize);

        let mut rover = Rover::new(dimens.h);

        let entries = Self::read_dir(&path).unwrap().into_iter().map(|e| ListEntry::from_dir_entry(e));
        rover.reset(entries.collect());
        rover.set_selected(0);

        DirScraper {
            current_path: Some(path),
            should_exit: false,
            rover: rover,
            mode: Mode::Flow,
            terminal_dimens: dimens,
        }
    }

    pub fn run(&mut self, stdout: &mut StdoutLock) -> std::io::Result<()> {

        stdout
            .queue(EnterAlternateScreen)?
            .queue(cursor::Hide)?
            .queue(DisableLineWrap)?
            .queue(EnableMouseCapture)?;

        enable_raw_mode()?;

        let mut renderer = ListRenderer::new(self.terminal_dimens, stdout);

        loop {
            match read().unwrap() {
                Event::Key(event) if event.kind == KeyEventKind::Press => match event.code {
                    KeyCode::Char(c) => {
                        if event.modifiers.contains(KeyModifiers::CONTROL) {
                            match c {
                                'q' | 'Q' => self.should_exit = true,
                                'c' => self.mode = Mode::Command,
                                'f' => self.mode = Mode::Flow,
                                'k' => self.execute_entry().unwrap(),
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
                    KeyCode::Enter => self.execute_entry().unwrap(),
                    KeyCode::Esc => self.to_parent_entry().unwrap(),
                    _ => {}
                },
                Event::Resize(w, h) => {
                    self.terminal_dimens.resize(w as usize, h as usize);
                    renderer.resize(w as usize, h as usize);
                    // TODO: reset rover.height
                }
                _ => {}
            }

            if let Some(selected) = self.rover.selected_mut() {
                selected.is_selected = true;
            }

            self.rover.render(&mut renderer);

            if let Some(selected) = self.rover.selected_mut() {
                selected.is_selected = false;
            }

            if self.should_exit {
                break;
            }
        }

        stdout
            .queue(LeaveAlternateScreen)?
            .queue(cursor::Show)?
            .queue(EnableLineWrap)?
            .queue(DisableMouseCapture)?
            .flush()?;

        disable_raw_mode()?;

        Ok(())
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

        let entries = Self::read_dir(&selected_path).unwrap().into_iter().map(|e| ListEntry::from_dir_entry(e));
        self.current_path = Some(selected_path.to_path_buf());
        // self.pivot = 0;

        self.rover.reset(entries.collect());

        Ok(())
    }

    fn execute_entry(&mut self) -> Result<(), String> {
        let selected_path = self.rover.selected_ref().unwrap().path();

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
    // dimens: Rect,
}

trait Component {
    fn render(&self, w: &mut impl Write);
}

struct ListEntry {
    dir_entry: DirEntry,
    pub is_selected: bool,
}

impl ListEntry {
    fn from_dir_entry(dir_entry: DirEntry) -> Self {
        Self {
            dir_entry,
            is_selected: false,
        }
    }
}

impl Deref for ListEntry {
    type Target = DirEntry;

    fn deref(&self) -> &Self::Target {
        &self.dir_entry
    }
}

impl Component for ListEntry {
    fn render(&self, w: &mut impl Write) {
        let prefix = if self.is_selected { ">\t" } else { "\t" };
        let target = format!("{}{}", prefix,self.dir_entry.file_name().to_str().unwrap());
        w.write(target.as_bytes()).unwrap();
    }
}

struct ListRenderer<'a, 'lock> {
    // TODO: wrap stdout with something that implements Write
    stdout: &'a mut StdoutLock<'lock>,
    bounds: Rect,
}

impl<'a, 'lock> ListRenderer<'a, 'lock> {
    fn new(bounds: Rect, stdout: &'a mut StdoutLock<'lock>) -> Self {
        Self { stdout, bounds }
    }

    fn resize(&mut self, w: usize, h: usize) {
        self.bounds.resize(w, h);
    }
}

impl Renderer for ListRenderer<'_, '_> {
    fn render<'a, I, T>(&mut self, components: I)
    where
        I: Iterator<Item = &'a T>,
        T: Component + 'a,
    {
       self.stdout.queue(Clear(ClearType::All)).unwrap();
        self.stdout
            .queue(BeginSynchronizedUpdate)
            .unwrap()
            .queue(MoveTo(0, 0))
            .unwrap();

        for (i, c) in components.enumerate() {
            c.render(self.stdout);
            self.stdout.queue(MoveTo(0, i.try_into().unwrap())).unwrap();
        }

        self.stdout
            .queue(EndSynchronizedUpdate)
            .unwrap()
            .flush()
            .unwrap();
    }
}

trait Renderer {
    fn render<'a, I, T>(&mut self, components: I)
    where
        I: Iterator<Item = &'a T>,
        T: Component + 'a;
}

struct Rover<C>
where
    C: Component,
    // R: Renderer,
{
    components: Option<Vec<C>>,
    ctx: Context,
    // renderer: Option<R>,
}

// impl<C: Component, R: Renderer> Rover<C, R> {
impl<C: Component> Rover<C> {
    pub fn new(height: usize) -> Self {
        Rover {
            components: None,
            ctx: Context {
                offset: 0,
                pivot: None,
                max_visible_rows: height,
                // dimens,
            },
            // renderer: Some(renderer),
        }
    }

    pub fn reset(&mut self, new_components: Vec<C>) {
        self.components = Some(new_components);
    }

    fn render(&mut self, r: &mut impl Renderer) {
        let offset = self.ctx.offset;
        let max_rows = self.ctx.max_visible_rows;

        let components = self
            .components
            .as_ref()
            .unwrap()
            .iter()
            .skip(offset)
            .take(max_rows);

        r.render(components);
    }

    fn selected_ref(&self) -> Option<&C> {
        Some(self.components.as_ref()?.get(self.ctx.pivot?)?)
    }

    fn selected_mut(&mut self) -> Option<&mut C> {
        Some(self.components.as_mut()?.get_mut(self.ctx.pivot?)?)
    }
    

    fn set_selected(&mut self, idx: usize) {
        assert!((0..self.len() - 1).contains(&idx));
        self.ctx.pivot = Some(idx);
    }

    // fn resize(&mut self, w: usize, h: usize) {
    //     self.ctx.dimens.resize(w, h);
    // }

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
        self.components
            .as_ref()
            .map(|c| c.len())
            .unwrap_or_default()
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

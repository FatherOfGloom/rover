use core::fmt;
use std::cell::RefCell;
use std::cmp::min;
use std::ffi::OsStr;
use std::fs::{self, DirEntry};
use std::io::{self, Stdout, StdoutLock, Write, stdout};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::usize;

use crossterm::cursor::{self, MoveTo, MoveToNextLine};
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

impl Component for (bool, DirEntry) {
    fn render(&self, out: &mut impl Write) {
        out.write(self.1.file_name().to_str().unwrap().as_bytes())
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
    pub fn init(path: PathBuf) -> io::Result<Self> {
        let (w, h) = terminal::size()?;
        let dimens = Rect::new(0, 0, w as usize, h as usize);

        let mut rover = Rover::new(dimens.h);

        let entries = Self::read_dir(&path);

        rover.reset(entries.unwrap());
        rover.set_selected(0);

        Ok(DirScraper {
            current_path: Some(path),
            should_exit: false,
            rover: rover,
            mode: Mode::Flow,
            terminal_dimens: dimens,
        })
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

            // if let Some(selected) = self.rover.selected_mut() {
            //     selected.is_selected = true;
            // }

            self.rover.render(&mut renderer);

            // if let Some(selected) = self.rover.selected_mut() {
            //     selected.is_selected = false;
            // }

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

    fn read_dir(path: &Path) -> Result<Vec<ListEntry>, String> {
        if !path.exists() {
            return Err(format!("Given path doesn't exist: '{}'.", path.display()));
        }

        if !path.is_dir() {
            return Err(format!(
                "Given path is not a directory: '{}'.",
                path.display()
            ));
        }

        let mut v = vec![];

        v.push(ListEntry::parent(path.parent().unwrap_or(path).to_path_buf()));

        for e in fs::read_dir(path).unwrap() {
            v.push(ListEntry::from_dir_entry(e.unwrap().path()).unwrap());
        }

        Ok(v)
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

        let entries = Self::read_dir(&selected_path);

        self.current_path = Some(selected_path.to_path_buf());
        self.rover.reset(entries?);
        self.rover.set_selected(0);

        Ok(())
    }

    fn execute_entry(&mut self) -> Result<(), String> {
        let selected =  match self.rover.selected_ref() {
            Some(r) => r,
            None => return Ok(()),
        };

        let kind = selected.kind();

        // I surrender to borrowing rules by cloning this bitch
        let selected = selected.to_path_buf();

        match kind {
            // let prev_pivot = self.pivot;
            // self.push_undo_pivot(prev_pivot);
            ListEntryKind::Dir | ListEntryKind::Parent => self.goto(&selected)?,
            ListEntryKind::File => {
                opener::open(selected.display().to_string()).map_err(|e| {
                    format!("Error opening the file '{}': {}", selected.display(), e)
                })?;
            }
            // ListEntryKind::Parent => todo!(),
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
    dir_entry: PathBuf,
    kind: ListEntryKind,
}

#[derive(Clone, Copy)]
enum ListEntryKind {
    Dir,
    File,
    Parent,
}

impl ListEntry {
    fn from_dir_entry(dir_entry: PathBuf) -> io::Result<Self> {
        let kind = if dir_entry.is_dir() {
            ListEntryKind::Dir
        } else {
            ListEntryKind::File
        };

        Ok(Self { dir_entry, kind })
    }

    fn parent(dir_entry: PathBuf) -> Self {
        Self { dir_entry, kind: ListEntryKind::Parent }
    }

    fn kind(&self) -> ListEntryKind {
        self.kind
    }
}

impl Deref for ListEntry {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.dir_entry
    }
}

impl Component for ListEntry {
    fn render(&self, w: &mut impl Write) {
        // let prefix = if self.is_selected { ">\t" } else { "\t" };
        // let target = format!("{}{}", prefix, self.dir_entry.file_name().to_str().unwrap());
        // w.write(target.as_bytes()).unwrap();

        // TODO: find a better solution when there is no file_name
        // Probably this can happen with a disc name on windows
        let binding = self.dir_entry.file_name().unwrap_or(OsStr::new("?"));
        let mut buffer = binding.to_str().unwrap().as_bytes().to_vec();
        let target = match self.kind() {
            ListEntryKind::Dir => {
                buffer.extend_from_slice(&b"/"[..]);
                buffer.as_slice()
            }
            ListEntryKind::File => buffer.as_slice(),
            ListEntryKind::Parent => &b"../"[..],
        };
        w.write(target).unwrap();
    }
}

struct ListRenderer<'a, 'lock> {
    writer: ListWriter<'a, 'lock>,
    bounds: Rect,
}

impl<'a, 'lock> ListRenderer<'a, 'lock> {
    fn new(bounds: Rect, stdout: &'a mut StdoutLock<'lock>) -> Self {
        Self {
            bounds,
            writer: ListWriter::new(stdout, bounds.w),
        }
    }

    fn resize(&mut self, w: usize, h: usize) {
        self.bounds.resize(w, h);
    }

    fn stdout(&mut self) -> &mut StdoutLock<'lock> {
        self.writer.lock()
    }
}

struct ListWriter<'a, 'lock> {
    lock: &'a mut StdoutLock<'lock>,
    is_selected: bool,
    line: usize,
    max_len: usize,
}

impl<'a, 'lock> ListWriter<'a, 'lock> {
    fn new(lock: &'a mut StdoutLock<'lock>, max_len: usize) -> Self {
        Self {
            lock,
            max_len,
            is_selected: false,
            line: 0,
        }
    }

    fn next_line(&mut self) {
        self.lock
            .queue(Clear(ClearType::UntilNewLine))
            .unwrap()
            .queue(MoveToNextLine(1))
            .unwrap();

        self.line += 1;
    }

    fn lock(&mut self) -> &mut StdoutLock<'lock> {
        self.lock
    }

    fn is_selected(&self) -> bool {
        self.is_selected
    }

    fn set_selection(&mut self, selection: bool) {
        self.is_selected = selection;
    }

    fn unselect(&mut self) {
        self.is_selected = false;
    }
}

impl Write for ListWriter<'_, '_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let prefix = if self.is_selected() { b"> " } else { b"  " };
        let mut target = prefix.to_vec();
        target.extend_from_slice(buf);
        let res = self.lock.write(&target[..min(target.len(), self.max_len)]);
        self.next_line();
        res
    }

    fn flush(&mut self) -> io::Result<()> {
        self.lock.flush()
    }
}

struct SelectionGuard<'a, 'lock>(&'a mut ListWriter<'a, 'lock>);

impl<'a, 'lock> Deref for SelectionGuard<'a, 'lock> {
    type Target = ListWriter<'a, 'lock>;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl Drop for SelectionGuard<'_, '_> {
    fn drop(&mut self) {
        self.0.unselect();
    }
}

impl Renderer for ListRenderer<'_, '_> {
    fn render<'a, I, T>(&mut self, components: I)
    where
        I: Iterator<Item = (bool, &'a T)>,
        T: Component + 'a,
    {
        self.stdout().queue(Clear(ClearType::All)).unwrap();
        self.stdout()
            .queue(BeginSynchronizedUpdate)
            .unwrap()
            .queue(MoveTo(0, 0))
            .unwrap();

        for (selected, c) in components {
            self.writer.set_selection(selected);

            c.render(&mut self.writer);

            self.writer.unselect();
        }

        self.stdout()
            .queue(EndSynchronizedUpdate)
            .unwrap()
            .flush()
            .unwrap();
    }
}

trait Renderer {
    fn render<'a, I, T>(&mut self, components: I)
    where
        I: Iterator<Item = (bool, &'a T)>,
        T: Component + 'a;
}

// struct Rover<'a, C, R>
struct Rover<C>
where
    C: Component,
    // R: Renderer,
{
    components: Option<Vec<C>>,
    ctx: Context,
    // r: Option<&'a mut R>,
}

// impl<C: Component, R: Renderer> Rover<C, R> {
impl<C: Component> Rover<C> {
    // impl<'a, C: Component, R: Renderer> Rover<'a, C, R> {
    // pub fn new(height: usize, r: &'a mut R) -> Self {
    pub fn new(height: usize) -> Self {
        Rover {
            components: None,
            ctx: Context {
                offset: 0,
                pivot: None,
                max_visible_rows: height,
                // dimens,
            },
            // r: Some(r),
        }
    }

    pub fn reset(&mut self, new_components: Vec<C>) {
        self.components = Some(new_components);
    }

    fn render(&mut self, r: &mut impl Renderer) {
        let offset = self.ctx.offset;
        let max_rows = self.ctx.max_visible_rows;
        let pivot = self.ctx.pivot;

        let components = self
            .components
            .as_ref()
            .unwrap()
            .iter()
            .skip(offset)
            .take(max_rows)
            .enumerate()
            .map(|(idx, c)| (pivot.map(|p| p == idx).unwrap_or_default(), c));

        r.render(components);
    }

    // fn update_selection(&mut self) {
    //     if let Some(pivot) = self.ctx.pivot {
    //         _ = self.r.as_mut().map(|r| r.update_selection(pivot));
    //     }
    // }

    fn selected_ref(&self) -> Option<&C> {
        Some(self.components.as_ref()?.get(self.ctx.pivot?)?)
    }

    fn selected_mut(&mut self) -> Option<&mut C> {
        Some(self.components.as_mut()?.get_mut(self.ctx.pivot?)?)
    }

    fn set_selected(&mut self, idx: usize) {
        let range = 0..self.len();
        assert!(range.contains(&idx), "{}", format!("idx: {} range: {:?}", idx, range));
        self.ctx.pivot = Some(idx);
    }

    fn resize(&mut self, w: usize, h: usize) {
        // TODO: update height + ctx
        todo!();
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

        // self.update_selection();
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
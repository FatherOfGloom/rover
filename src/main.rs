use crossterm::terminal;

fn main() {
    terminal::enable_raw_mode().unwrap();
    rover::run().unwrap();
    terminal::disable_raw_mode().unwrap();
}
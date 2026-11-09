//! Setu's window: connect to the headset, see what is playing, pick the output.
//!
//! Release builds are a GUI app with no console; debug builds keep one for logs. `setu-cli` is
//! the command-line front end.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod brand;
mod ui;

fn main() {
    ui::run();
}

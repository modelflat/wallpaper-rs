#![feature(drain_filter)]

#![windows_subsystem = "windows"]

use serde::{Serialize, Deserialize};
use web_view::*;
use winapi::shared::windef::HWND;

mod shellwords;
mod wallpaper;

#[derive(Debug, Serialize, Deserialize)]
struct UserData { }

#[derive(Debug, Serialize, Deserialize)]
struct Window {
    hwnd: u32,
    title: String,
}

impl Window {

    fn from_handles(handles: Vec<HWND>) -> Vec<Window> {
        handles.into_iter().map(|hwnd| {
            Window { title: wallpaper::get_window_name(hwnd), hwnd: hwnd as u32 }
        }).collect()
    }

}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Command<'a> {
    UpdateRunningWallpapers {},
    UpdateActiveWindows {},
    NewFromSelectedActiveWindow { 
        selected: u32,
        properties: wallpaper::WallpaperProperties,
    },
    NewFromCustomCommand { 
        command: &'a str, 
        selector: wallpaper::WindowSelector<'a>, 
        properties: wallpaper::WallpaperProperties,
    },
    TerminateRunningWallpaper { selected: u32 },
}

fn command_from_str(command: &str) -> Result<std::process::Command, shellwords::MismatchedQuotes> {
    let mut iter = shellwords::split(command)?.into_iter();
    let mut command = std::process::Command::new(iter.next().unwrap());
    for word in iter {
        command.arg(word);
    }
    Ok(command)
}

fn handler(web_view: &mut WebView<UserData>, arg: &str) -> WVResult {
    let wp = wallpaper::Engine::new().expect("Failed to create wallpaper engine");
    let arg: Command = serde_json::from_str(arg).unwrap();
    
    match arg {
        Command::UpdateActiveWindows {} => {
            let windows = Window::from_handles(wallpaper::list_windows());
            let windows_stringified = serde_json::to_string(&windows).unwrap();
            web_view.eval(&format!("window._updateList('activeWindows', {})", windows_stringified)).unwrap();
        },
        Command::UpdateRunningWallpapers {} => {
            let windows = Window::from_handles(wp.list_active());
            let windows_stringified = serde_json::to_string(&windows).unwrap();
            web_view.eval(&format!("window._updateList('runningWallpapers', {})", windows_stringified)).unwrap();
        },
        Command::NewFromSelectedActiveWindow { selected, properties } => {
            let result = wp.add_window_by_handle(selected as winapi::shared::windef::HWND, properties);
            if !result {
                eprintln!("Failed to add window");
            }
        },
        Command::NewFromCustomCommand { command, selector, properties } => {
            if !command.trim_start().trim_end().is_empty() {
                match command_from_str(command) {
                    Ok(mut command) => {
                        wp.add_window(Some(&mut command), selector, properties, 50, 100);
                    },
                    Err(error) => {
                        eprintln!("Error parsing command {:?}", error);
                    }
                }
            }
        },
        Command::TerminateRunningWallpaper { selected } => {
            wp.remove_wallpaper(selected as HWND);
        }
    }
    Ok(())
}

fn main() {
    let html_content = include_str!("../html/index.html");
    
    web_view::builder()
        .title("wallpaper")
        .content(Content::Html(html_content))
        .size(640, 480)
        .resizable(false)
        .debug(true)
        .user_data(UserData {})
        .invoke_handler(handler)
        .run()
        .unwrap();
}

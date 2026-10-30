#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    #[cfg(windows)]
    {
        let code = allblack11::start();
        std::process::exit(code);
    }
    #[cfg(not(windows))]
    {
        eprintln!("AllBlack11 is a Windows 11 tray application and does not run on this OS.");
        std::process::exit(1);
    }
}

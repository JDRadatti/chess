//! Prints what the terminal reports about image support and cell size.

use ratatui_image::picker::Picker;

fn main() {
    match Picker::from_query_stdio() {
        Ok(picker) => println!(
            "protocol: {:?}\nfont size: {:?}\ncapabilities: {:?}",
            picker.protocol_type(),
            picker.font_size(),
            picker.capabilities()
        ),
        Err(err) => println!("query failed: {err}"),
    }
    println!(
        "window size: {:?}",
        ratatui::crossterm::terminal::window_size()
    );
    for var in ["TERM", "TERM_PROGRAM", "GDK_SCALE", "QT_SCALE_FACTOR"] {
        println!("{var}={:?}", std::env::var(var).ok());
    }
}

// Reports the terminal size ratatui/crossterm detects, plus what a full-screen
// ratatui Frame area comes out as. Writes to /tmp/gori_size.txt and exits.
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
fn main() -> std::io::Result<()> {
    let (c, r) = ratatui::crossterm::terminal::size()?;
    let mut term = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
    let area = term.get_frame().area();
    std::fs::write(
        "/tmp/gori_size.txt",
        format!(
            "crossterm size cols={c} rows={r}\nframe area w={} h={}\n",
            area.width, area.height
        ),
    )?;
    Ok(())
}

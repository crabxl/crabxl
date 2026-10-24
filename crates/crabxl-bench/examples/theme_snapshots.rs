//! Verify independent theme holders and exact immutable bytes after dropping the source.
use crabxl::Theme;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let size: usize = std::env::args()
        .nth(1)
        .ok_or("Usage: theme_snapshots <bytes>")?
        .parse()?;
    if size == 0 || size > 16 * 1024 * 1024 {
        return Err("Invalid bounded theme size".into());
    }
    let theme = Theme::from_bytes(vec![b'x'; size].into_boxed_slice());
    let mut snapshots = Vec::new();
    snapshots.try_reserve_exact(64)?;
    for _ in 0..64 {
        snapshots.push(theme.clone());
    }
    drop(theme);
    for theme in &snapshots {
        if theme.bytes().len() != size || theme.bytes().iter().any(|byte| *byte != b'x') {
            return Err("Invalid snapshot bytes".into());
        }
    }
    println!("{}", size * snapshots.len());
    Ok(())
}

use serde::{Deserialize, Serialize};

use super::Error;

const MAX_RECENT_COLORS: usize = 10;

#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
struct RecentColors {
    colors: Vec<[u8; 4]>,
}

impl RecentColors {
    fn remember(&mut self, color: [u8; 4]) {
        self.colors.retain(|saved| *saved != color);
        self.colors.insert(0, color);
        self.colors.truncate(MAX_RECENT_COLORS);
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_recent_colors() -> std::result::Result<Vec<[u8; 4]>, Error> {
    let config = koharu_config::load::<RecentColors>("recent_colors")?;
    Ok(config.read()?.colors.clone())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn remember_color(color: [u8; 4]) -> std::result::Result<Vec<[u8; 4]>, Error> {
    let config = koharu_config::load::<RecentColors>("recent_colors")?;
    let mut preferences = config.write()?;
    preferences.remember(color);
    let colors = preferences.colors.clone();
    preferences.save()?;
    Ok(colors)
}

#[cfg(test)]
mod tests {
    use super::{MAX_RECENT_COLORS, RecentColors};

    #[test]
    fn persists_ten_unique_colors_in_last_applied_order() {
        let mut preferences = RecentColors::default();
        for red in 0..12 {
            preferences.remember([red, 0, 0, 255]);
        }
        assert_eq!(preferences.colors.len(), MAX_RECENT_COLORS);
        assert_eq!(preferences.colors[0], [11, 0, 0, 255]);
        assert_eq!(preferences.colors[9], [2, 0, 0, 255]);

        preferences.remember([5, 0, 0, 255]);
        assert_eq!(preferences.colors.len(), MAX_RECENT_COLORS);
        assert_eq!(preferences.colors[0], [5, 0, 0, 255]);
        assert_eq!(preferences.colors[1], [11, 0, 0, 255]);
        assert_eq!(
            preferences
                .colors
                .iter()
                .filter(|color| **color == [5, 0, 0, 255])
                .count(),
            1
        );

        let saved = serde_json::to_string(&preferences).unwrap();
        let loaded: RecentColors = serde_json::from_str(&saved).unwrap();
        assert_eq!(loaded.colors, preferences.colors);
    }
}

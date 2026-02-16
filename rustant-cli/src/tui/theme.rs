//! Theme system for the Rustant TUI.
//!
//! Provides dark and light color palettes, loaded from UiConfig.theme.

use ratatui::style::{Color, Modifier, Style};

/// Complete color theme for the TUI.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Theme {
    pub name: String,

    // Base colors
    pub bg: Color,
    pub fg: Color,
    pub accent: Color,

    // Message colors
    pub user_msg_fg: Color,
    pub assistant_msg_fg: Color,
    pub system_msg_fg: Color,
    pub tool_call_fg: Color,
    pub tool_result_fg: Color,

    // Status colors
    pub error_fg: Color,
    pub warning_fg: Color,
    pub success_fg: Color,
    pub info_fg: Color,

    // UI chrome
    pub header_bg: Color,
    pub header_fg: Color,
    pub status_bar_bg: Color,
    pub status_bar_fg: Color,
    pub border_color: Color,
    pub selection_bg: Color,
    pub sidebar_bg: Color,

    // Diff colors
    pub diff_add_fg: Color,
    pub diff_remove_fg: Color,
    pub diff_context_fg: Color,

    // Syntax highlighting theme name (for syntect)
    pub syntect_theme: String,
}

impl Theme {
    /// Create the default dark theme.
    pub fn dark() -> Self {
        Self {
            name: "dark".to_string(),
            bg: Color::Rgb(30, 30, 46),
            fg: Color::Rgb(205, 214, 244),
            accent: Color::Rgb(78, 205, 126),

            user_msg_fg: Color::Rgb(78, 205, 126),
            assistant_msg_fg: Color::Rgb(166, 227, 161),
            system_msg_fg: Color::Rgb(127, 132, 156),
            tool_call_fg: Color::Rgb(249, 226, 175),
            tool_result_fg: Color::Rgb(180, 190, 254),

            error_fg: Color::Rgb(243, 139, 168),
            warning_fg: Color::Rgb(250, 179, 135),
            success_fg: Color::Rgb(166, 227, 161),
            info_fg: Color::Rgb(78, 205, 126),

            header_bg: Color::Rgb(24, 24, 37),
            header_fg: Color::Rgb(205, 214, 244),
            status_bar_bg: Color::Rgb(24, 24, 37),
            status_bar_fg: Color::Rgb(166, 173, 200),
            border_color: Color::Rgb(69, 71, 90),
            selection_bg: Color::Rgb(69, 71, 90),
            sidebar_bg: Color::Rgb(24, 24, 37),

            diff_add_fg: Color::Rgb(166, 227, 161),
            diff_remove_fg: Color::Rgb(243, 139, 168),
            diff_context_fg: Color::Rgb(127, 132, 156),

            syntect_theme: "base16-ocean.dark".to_string(),
        }
    }

    /// Create the light theme.
    pub fn light() -> Self {
        Self {
            name: "light".to_string(),
            bg: Color::Rgb(239, 241, 245),
            fg: Color::Rgb(76, 79, 105),
            accent: Color::Rgb(30, 102, 245),

            user_msg_fg: Color::Rgb(30, 102, 245),
            assistant_msg_fg: Color::Rgb(64, 160, 43),
            system_msg_fg: Color::Rgb(140, 143, 161),
            tool_call_fg: Color::Rgb(223, 142, 29),
            tool_result_fg: Color::Rgb(114, 135, 253),

            error_fg: Color::Rgb(210, 15, 57),
            warning_fg: Color::Rgb(254, 100, 11),
            success_fg: Color::Rgb(64, 160, 43),
            info_fg: Color::Rgb(30, 102, 245),

            header_bg: Color::Rgb(220, 224, 232),
            header_fg: Color::Rgb(76, 79, 105),
            status_bar_bg: Color::Rgb(220, 224, 232),
            status_bar_fg: Color::Rgb(92, 95, 119),
            border_color: Color::Rgb(172, 176, 190),
            selection_bg: Color::Rgb(188, 192, 204),
            sidebar_bg: Color::Rgb(230, 233, 239),

            diff_add_fg: Color::Rgb(64, 160, 43),
            diff_remove_fg: Color::Rgb(210, 15, 57),
            diff_context_fg: Color::Rgb(140, 143, 161),

            syntect_theme: "base16-ocean.light".to_string(),
        }
    }

    /// Load a theme by name from config. Falls back to dark.
    pub fn from_name(name: &str) -> Self {
        match name {
            "light" => Self::light(),
            _ => Self::dark(),
        }
    }

    // -- Convenience style constructors --

    pub fn base_style(&self) -> Style {
        Style::default().fg(self.fg).bg(self.bg)
    }

    pub fn header_style(&self) -> Style {
        Style::default().fg(self.header_fg).bg(self.header_bg)
    }

    pub fn status_bar_style(&self) -> Style {
        Style::default()
            .fg(self.status_bar_fg)
            .bg(self.status_bar_bg)
    }

    pub fn user_message_style(&self) -> Style {
        Style::default()
            .fg(self.user_msg_fg)
            .add_modifier(Modifier::BOLD)
    }

    pub fn assistant_message_style(&self) -> Style {
        Style::default().fg(self.assistant_msg_fg)
    }

    pub fn tool_call_style(&self) -> Style {
        Style::default().fg(self.tool_call_fg)
    }

    pub fn tool_result_style(&self) -> Style {
        Style::default().fg(self.tool_result_fg)
    }

    pub fn error_style(&self) -> Style {
        Style::default()
            .fg(self.error_fg)
            .add_modifier(Modifier::BOLD)
    }

    pub fn warning_style(&self) -> Style {
        Style::default().fg(self.warning_fg)
    }

    pub fn success_style(&self) -> Style {
        Style::default().fg(self.success_fg)
    }

    pub fn border_style(&self) -> Style {
        Style::default().fg(self.border_color)
    }

    pub fn sidebar_style(&self) -> Style {
        Style::default().fg(self.fg).bg(self.sidebar_bg)
    }

    /// Return a context fill color: green < 50%, yellow 50-80%, red > 80%.
    pub fn context_gauge_color(&self, ratio: f32) -> Color {
        if ratio < 0.5 {
            self.success_fg
        } else if ratio < 0.8 {
            self.warning_fg
        } else {
            self.error_fg
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dark_theme_creation() {
        let theme = Theme::dark();
        assert_eq!(theme.name, "dark");
        assert_eq!(theme.bg, Color::Rgb(30, 30, 46));
        assert_eq!(theme.syntect_theme, "base16-ocean.dark");
    }

    #[test]
    fn test_light_theme_creation() {
        let theme = Theme::light();
        assert_eq!(theme.name, "light");
        assert_eq!(theme.bg, Color::Rgb(239, 241, 245));
        assert_eq!(theme.syntect_theme, "base16-ocean.light");
    }

    #[test]
    fn test_from_name_dark() {
        let theme = Theme::from_name("dark");
        assert_eq!(theme.name, "dark");
    }

    #[test]
    fn test_from_name_light() {
        let theme = Theme::from_name("light");
        assert_eq!(theme.name, "light");
    }

    #[test]
    fn test_from_name_unknown_defaults_to_dark() {
        let theme = Theme::from_name("solarized");
        assert_eq!(theme.name, "dark");
    }

    #[test]
    fn test_base_style() {
        let theme = Theme::dark();
        let style = theme.base_style();
        assert_eq!(style.fg, Some(theme.fg));
        assert_eq!(style.bg, Some(theme.bg));
    }

    #[test]
    fn test_context_gauge_color_green() {
        let theme = Theme::dark();
        assert_eq!(theme.context_gauge_color(0.3), theme.success_fg);
    }

    #[test]
    fn test_context_gauge_color_yellow() {
        let theme = Theme::dark();
        assert_eq!(theme.context_gauge_color(0.6), theme.warning_fg);
    }

    #[test]
    fn test_context_gauge_color_red() {
        let theme = Theme::dark();
        assert_eq!(theme.context_gauge_color(0.9), theme.error_fg);
    }

    #[test]
    fn test_user_message_style_is_bold() {
        let theme = Theme::dark();
        let style = theme.user_message_style();
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }
}

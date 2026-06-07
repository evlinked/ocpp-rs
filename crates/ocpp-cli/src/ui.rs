//! # CLI User Interface Module
//!
//! This module provides user interface utilities and interactive components
//! for the OCPP CLI tool.

use anyhow::Result;
use colored::*;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::time::Duration;

/// CLI theme configuration
pub struct CliTheme {
    /// Primary color for highlights
    pub primary_color: Color,
    /// Secondary color for info
    pub secondary_color: Color,
    /// Error color
    pub error_color: Color,
    /// Success color
    pub success_color: Color,
    /// Warning color
    pub warning_color: Color,
}

impl Default for CliTheme {
    fn default() -> Self {
        Self {
            primary_color: Color::Cyan,
            secondary_color: Color::Blue,
            error_color: Color::Red,
            success_color: Color::Green,
            warning_color: Color::Yellow,
        }
    }
}

/// Progress bar utilities
pub struct ProgressBarBuilder {
    length: Option<u64>,
    message: Option<String>,
    style_template: Option<String>,
}

impl ProgressBarBuilder {
    /// Create new progress bar builder
    pub fn new() -> Self {
        Self {
            length: None,
            message: None,
            style_template: None,
        }
    }

    /// Set progress bar length
    pub fn length(mut self, length: u64) -> Self {
        self.length = Some(length);
        self
    }

    /// Set initial message
    pub fn message<S: Into<String>>(mut self, message: S) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Set custom style template
    pub fn style_template<S: Into<String>>(mut self, template: S) -> Self {
        self.style_template = Some(template.into());
        self
    }

    /// Build the progress bar
    pub fn build(self) -> Result<ProgressBar> {
        let pb = if let Some(length) = self.length {
            ProgressBar::new(length)
        } else {
            ProgressBar::new_spinner()
        };

        let template = self.style_template.unwrap_or_else(|| {
            if self.length.is_some() {
                "{spinner:.green} [{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}"
                    .to_string()
            } else {
                "{spinner:.blue} {msg}".to_string()
            }
        });

        pb.set_style(
            ProgressStyle::default_bar()
                .template(&template)?
                .progress_chars("#>-"),
        );

        if let Some(message) = self.message {
            pb.set_message(message);
        }

        Ok(pb)
    }
}

impl Default for ProgressBarBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Interactive menu utilities
pub struct InteractiveMenu {
    theme: ColorfulTheme,
}

impl InteractiveMenu {
    /// Create new interactive menu
    pub fn new() -> Self {
        Self {
            theme: ColorfulTheme::default(),
        }
    }

    /// Show selection menu
    pub fn select_from_list<T: std::fmt::Display>(
        &self,
        prompt: &str,
        items: &[T],
    ) -> Result<usize> {
        let selection = Select::with_theme(&self.theme)
            .with_prompt(prompt)
            .items(items)
            .interact()?;

        Ok(selection)
    }

    /// Show confirmation dialog
    pub fn confirm(&self, prompt: &str, default: Option<bool>) -> Result<bool> {
        let mut confirm = Confirm::with_theme(&self.theme).with_prompt(prompt);

        if let Some(default_value) = default {
            confirm = confirm.default(default_value);
        }

        Ok(confirm.interact()?)
    }

    /// Get text input from user
    pub fn input<T>(&self, prompt: &str, default: Option<T>) -> Result<T>
    where
        T: Clone + std::fmt::Display + std::str::FromStr,
        T::Err: std::fmt::Display + std::fmt::Debug,
    {
        let mut input = Input::with_theme(&self.theme).with_prompt(prompt);

        if let Some(default_value) = default {
            input = input.default(default_value);
        }

        Ok(input.interact_text()?)
    }

    /// Show multi-select menu
    pub fn multi_select<T: std::fmt::Display>(
        &self,
        prompt: &str,
        items: &[T],
        defaults: &[bool],
    ) -> Result<Vec<usize>> {
        use dialoguer::MultiSelect;

        let selection = MultiSelect::with_theme(&self.theme)
            .with_prompt(prompt)
            .items(items)
            .defaults(defaults)
            .interact()?;

        Ok(selection)
    }
}

impl Default for InteractiveMenu {
    fn default() -> Self {
        Self::new()
    }
}

/// Status display utilities
pub struct StatusDisplay {
    #[allow(dead_code)]
    theme: CliTheme,
}

impl StatusDisplay {
    /// Create new status display
    pub fn new(theme: CliTheme) -> Self {
        Self { theme }
    }

    /// Display success message
    pub fn success(&self, message: &str) {
        println!("✅ {}", message.bright_green());
    }

    /// Display error message
    pub fn error(&self, message: &str) {
        println!("❌ {}", message.bright_red());
    }

    /// Display warning message
    pub fn warning(&self, message: &str) {
        println!("⚠️  {}", message.bright_yellow());
    }

    /// Display info message
    pub fn info(&self, message: &str) {
        println!("ℹ️  {}", message.bright_blue());
    }

    /// Display debug message
    pub fn debug(&self, message: &str) {
        println!("🐛 {}", message.bright_magenta());
    }

    /// Display section header
    pub fn section_header(&self, title: &str) {
        println!("\n{}", title.bright_cyan().bold());
        println!("{}", "━".repeat(title.len()).bright_cyan());
    }

    /// Display key-value pair
    pub fn key_value(&self, key: &str, value: &str) {
        println!("   {}: {}", key.bright_white(), value.bright_yellow());
    }

    /// Display connection status
    pub fn connection_status(&self, connected: bool, url: &str) {
        if connected {
            println!(
                "🟢 {} - {}",
                "Connected".bright_green().bold(),
                url.bright_white()
            );
        } else {
            println!(
                "🔴 {} - {}",
                "Disconnected".bright_red().bold(),
                url.bright_white()
            );
        }
    }

    /// Display test result
    pub fn test_result(&self, name: &str, success: bool, duration_ms: u64) {
        let status = if success {
            "PASS".bright_green()
        } else {
            "FAIL".bright_red()
        };

        println!(
            "   {} | {} | {}ms",
            status,
            name.bright_white(),
            duration_ms.to_string().bright_blue()
        );
    }

    /// Display connector status
    pub fn connector_status(&self, connector_id: u32, status: &str, cable_plugged: bool) {
        let cable_icon = if cable_plugged { "🔌" } else { "⚡" };
        let status_colored = match status {
            "Available" => status.bright_green(),
            "Preparing" | "Finishing" => status.bright_yellow(),
            "Charging" => status.bright_blue(),
            "Faulted" => status.bright_red(),
            "Unavailable" => status.bright_magenta(),
            _ => status.normal(),
        };

        println!(
            "   {} Connector {}: {}",
            cable_icon, connector_id, status_colored
        );
    }

    /// Display transaction info
    pub fn transaction_info(
        &self,
        transaction_id: i32,
        connector_id: u32,
        user: &str,
        energy_kwh: f64,
        duration: Duration,
    ) {
        println!(
            "⚡ Transaction {}",
            transaction_id.to_string().bright_yellow()
        );
        println!("   Connector: {}", connector_id.to_string().bright_cyan());
        println!("   User: {}", user.bright_white());
        println!(
            "   Energy: {} kWh",
            format!("{:.2}", energy_kwh).bright_green()
        );
        println!("   Duration: {}", format_duration(duration).bright_blue());
    }

    /// Display statistics table
    pub fn statistics_table(&self, stats: &HashMap<String, String>) {
        self.section_header("Statistics");

        for (key, value) in stats {
            self.key_value(key, value);
        }
    }
}

impl Default for StatusDisplay {
    fn default() -> Self {
        Self::new(CliTheme::default())
    }
}

/// Banner display utilities
pub struct BannerDisplay;

impl BannerDisplay {
    /// Display application banner
    pub fn show_banner() {
        println!(
            "{}",
            r#"
   ____   ____ ____  ____     ____ _     ___
  / __ \ / ___/ __ \|  _ \   / ___| |   |_ _|
 | |  | | |  | |  | | |_) | | |   | |    | |
 | |__| | |__| |__| |  __/  | |___| |___ | |
  \____/ \____\____/|_|      \____|_____|___|

        "#
            .bright_cyan()
        );
    }

    /// Display loading animation
    pub async fn show_loading(message: &str, duration: Duration) {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
                .template("{spinner:.blue} {msg}")
                .unwrap(),
        );
        pb.set_message(message.to_string());
        pb.enable_steady_tick(Duration::from_millis(120));

        tokio::time::sleep(duration).await;
        pb.finish_with_message(format!("✅ {}", message));
    }

    /// Display connection attempt
    pub async fn show_connection_attempt(url: &str) -> Result<bool> {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ ")
                .template("{spinner:.green} Connecting to {msg}")
                .unwrap(),
        );
        pb.set_message(url.to_string());
        pb.enable_steady_tick(Duration::from_millis(100));

        // Simulate connection attempt
        tokio::time::sleep(Duration::from_millis(2000)).await;

        // Simulate success/failure (90% success rate for demo)
        let success = rand::random::<f32>() < 0.9;

        if success {
            pb.finish_with_message(format!("✅ Connected to {}", url.bright_green()));
        } else {
            pb.finish_with_message(format!("❌ Failed to connect to {}", url.bright_red()));
        }

        Ok(success)
    }
}

/// Table display utilities
pub struct TableDisplay {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    column_widths: Vec<usize>,
}

impl TableDisplay {
    /// Create new table display
    pub fn new(headers: Vec<String>) -> Self {
        let column_widths = headers.iter().map(|h| h.len()).collect();

        Self {
            headers,
            rows: Vec::new(),
            column_widths,
        }
    }

    /// Add row to table
    pub fn add_row(&mut self, row: Vec<String>) {
        // Update column widths
        for (i, cell) in row.iter().enumerate() {
            if i < self.column_widths.len() {
                self.column_widths[i] = self.column_widths[i].max(cell.len());
            }
        }
        self.rows.push(row);
    }

    /// Display the table
    pub fn display(&self) {
        // Display headers
        self.print_separator();
        self.print_row(&self.headers, true);
        self.print_separator();

        // Display rows
        for row in &self.rows {
            self.print_row(row, false);
        }
        self.print_separator();
    }

    fn print_separator(&self) {
        print!("┌");
        for (i, width) in self.column_widths.iter().enumerate() {
            print!("{}", "─".repeat(width + 2));
            if i < self.column_widths.len() - 1 {
                print!("┬");
            }
        }
        println!("┐");
    }

    fn print_row(&self, row: &[String], is_header: bool) {
        print!("│");
        for (i, (cell, width)) in row.iter().zip(&self.column_widths).enumerate() {
            let formatted_cell = if is_header {
                format!(" {:^width$} ", cell, width = width)
            } else {
                format!(" {:<width$} ", cell, width = width)
            };

            if is_header {
                print!("{}", formatted_cell.bright_cyan().bold());
            } else {
                print!("{}", formatted_cell);
            }

            if i < self.column_widths.len() - 1 {
                print!("│");
            }
        }
        println!("│");
    }
}

/// Utility functions
pub mod utils {
    use super::*;

    /// Format duration in human-readable format
    pub fn format_duration(duration: Duration) -> String {
        let total_seconds = duration.as_secs();
        let hours = total_seconds / 3600;
        let minutes = (total_seconds % 3600) / 60;
        let seconds = total_seconds % 60;

        if hours > 0 {
            format!("{}h {}m {}s", hours, minutes, seconds)
        } else if minutes > 0 {
            format!("{}m {}s", minutes, seconds)
        } else {
            format!("{}s", seconds)
        }
    }

    /// Format bytes in human-readable format
    pub fn format_bytes(bytes: u64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        let mut size = bytes as f64;
        let mut unit_index = 0;

        while size >= 1024.0 && unit_index < UNITS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }

        if unit_index == 0 {
            format!("{} {}", bytes, UNITS[unit_index])
        } else {
            format!("{:.1} {}", size, UNITS[unit_index])
        }
    }

    /// Create a spinner with custom message
    pub fn create_spinner(message: &str) -> ProgressBar {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
                .template("{spinner:.blue} {msg}")
                .unwrap(),
        );
        pb.set_message(message.to_string());
        pb.enable_steady_tick(Duration::from_millis(120));
        pb
    }

    /// Display colorized JSON
    pub fn display_json(json: &serde_json::Value) {
        let formatted =
            serde_json::to_string_pretty(json).unwrap_or_else(|_| "Invalid JSON".to_string());

        for line in formatted.lines() {
            if line.trim().starts_with('"') && line.contains(':') {
                // Key-value pair
                let parts: Vec<&str> = line.splitn(2, ':').collect();
                if parts.len() == 2 {
                    print!("{}", parts[0].bright_cyan());
                    println!(":{}", parts[1]);
                } else {
                    println!("{}", line);
                }
            } else if line.trim().starts_with('"') {
                // String value
                println!("{}", line.bright_green());
            } else if line.trim().chars().next().is_some_and(|c| c.is_numeric()) {
                // Numeric value
                println!("{}", line.bright_yellow());
            } else {
                // Other (braces, brackets, etc.)
                println!("{}", line.bright_white());
            }
        }
    }

    /// Clear screen
    pub fn clear_screen() {
        print!("\x1B[2J\x1B[1;1H");
    }

    /// Move cursor to position
    pub fn move_cursor(row: u16, col: u16) {
        print!("\x1B[{};{}H", row, col);
    }
}

// Helper function for formatting duration (used above)
pub fn format_duration(duration: Duration) -> String {
    utils::format_duration(duration)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_theme_default() {
        let theme = CliTheme::default();
        assert_eq!(theme.primary_color, Color::Cyan);
        assert_eq!(theme.success_color, Color::Green);
    }

    #[test]
    fn test_progress_bar_builder() {
        let pb = ProgressBarBuilder::new()
            .length(100)
            .message("Testing")
            .build()
            .unwrap();

        assert_eq!(pb.length(), Some(100));
    }

    #[test]
    fn test_status_display() {
        let display = StatusDisplay::default();

        // These would normally print to stdout, but we can't easily test that
        // In a real scenario, you might use a mock writer or capture stdout
        display.success("Test success");
        display.error("Test error");
        display.warning("Test warning");
        display.info("Test info");
    }

    #[test]
    fn test_table_display() {
        let mut table = TableDisplay::new(vec![
            "Name".to_string(),
            "Status".to_string(),
            "Duration".to_string(),
        ]);

        table.add_row(vec![
            "Test 1".to_string(),
            "PASS".to_string(),
            "123ms".to_string(),
        ]);

        table.add_row(vec![
            "Test 2".to_string(),
            "FAIL".to_string(),
            "456ms".to_string(),
        ]);

        // Test that table structure is correct
        assert_eq!(table.headers.len(), 3);
        assert_eq!(table.rows.len(), 2);
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(utils::format_duration(Duration::from_secs(30)), "30s");
        assert_eq!(utils::format_duration(Duration::from_secs(90)), "1m 30s");
        assert_eq!(
            utils::format_duration(Duration::from_secs(3661)),
            "1h 1m 1s"
        );
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(utils::format_bytes(512), "512 B");
        assert_eq!(utils::format_bytes(1536), "1.5 KB");
        assert_eq!(utils::format_bytes(2097152), "2.0 MB");
    }

    #[test]
    fn test_format_json() {
        let json = serde_json::json!({
            "test": "value",
            "number": 42,
            "nested": {
                "key": "value"
            }
        });

        // This would normally print colored output
        // We're just testing that it doesn't panic
        utils::display_json(&json);
    }
}

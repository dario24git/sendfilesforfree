use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::process::{Command, Child};
use anyhow::Result;
use eframe::{egui, App, CreationContext};
use egui::{TextEdit, ScrollArea, RichText, TextStyle, Color32, Vec2, Frame};
use egui::style::Margin;
use std::io::BufRead;
use arboard::Clipboard;

// Color palette
struct AppColors;
impl AppColors {
    const BACKGROUND: Color32 = Color32::from_rgb(248, 249, 250);  // Light gray background
    const PRIMARY: Color32 = Color32::from_rgb(47, 128, 237);      // Main blue color
    const PRIMARY_LIGHT: Color32 = Color32::from_rgb(66, 133, 244); // Lighter blue for hover
    const SUCCESS: Color32 = Color32::from_rgb(40, 167, 69);       // Green for success states
    const DANGER: Color32 = Color32::from_rgb(220, 53, 69);        // Red for errors/stop
    const TEXT_PRIMARY: Color32 = Color32::from_rgb(33, 37, 41);   // Dark gray for main text
    const TEXT_SECONDARY: Color32 = Color32::from_rgb(108, 117, 125); // Medium gray for secondary text
    const TEXT_ON_COLOR: Color32 = Color32::WHITE;                 // White text on colored backgrounds
    const DISABLED: Color32 = Color32::from_rgb(173, 181, 189);    // Gray for disabled states
    const SURFACE: Color32 = Color32::WHITE;                       // White for cards/panels
}

/// GUI application state
pub struct SendmeApp {
    mode: AppMode,
    file_path: String,
    ticket: String,
    status: String,
    output: Arc<Mutex<String>>,
    extracted_ticket: Arc<Mutex<String>>,
    is_ticket_ready: Arc<Mutex<bool>>,
    command_running: Arc<Mutex<bool>>,
    is_sending: bool, // Track if a sending session is active
    child_process: Arc<Mutex<Option<Child>>>, // Store the child process
}

#[derive(PartialEq, Clone, Copy)]
enum AppMode {
    Send,
    Receive,
}

impl SendmeApp {
    pub fn new(_cc: &CreationContext<'_>) -> Self {
        Self {
            mode: AppMode::Send,
            file_path: String::new(),
            ticket: String::new(),
            status: String::new(),
            output: Arc::new(Mutex::new(String::new())),
            extracted_ticket: Arc::new(Mutex::new(String::new())),
            is_ticket_ready: Arc::new(Mutex::new(false)),
            command_running: Arc::new(Mutex::new(false)),
            is_sending: false,
            child_process: Arc::new(Mutex::new(None)),
        }
    }
}

impl Default for SendmeApp {
    fn default() -> Self {
        Self {
            mode: AppMode::Send,
            file_path: String::new(),
            ticket: String::new(),
            status: String::from("Ready"),
            output: Arc::new(Mutex::new(String::new())),
            extracted_ticket: Arc::new(Mutex::new(String::new())),
            is_ticket_ready: Arc::new(Mutex::new(false)),
            command_running: Arc::new(Mutex::new(false)),
            is_sending: false,
            child_process: Arc::new(Mutex::new(None)),
        }
    }
}

impl App for SendmeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Set custom style
        let mut style = egui::Style::default();
        style.spacing.item_spacing = Vec2::new(10.0, 15.0);
        style.spacing.window_margin = Margin::same(15.0);
        style.spacing.button_padding = Vec2::new(12.0, 6.0);
        style.visuals.widgets.noninteractive.bg_fill = AppColors::SURFACE;
        style.visuals.widgets.inactive.bg_fill = AppColors::SURFACE;
        style.visuals.widgets.active.bg_fill = AppColors::SURFACE;
        style.visuals.widgets.hovered.bg_fill = AppColors::SURFACE;
        style.visuals.extreme_bg_color = AppColors::BACKGROUND;
        style.visuals.widgets.noninteractive.fg_stroke.color = AppColors::TEXT_PRIMARY;
        style.visuals.widgets.inactive.fg_stroke.color = AppColors::TEXT_PRIMARY;
        style.visuals.widgets.hovered.fg_stroke.color = AppColors::TEXT_PRIMARY;
        style.visuals.widgets.active.fg_stroke.color = AppColors::TEXT_PRIMARY;
        ctx.set_style(style);

        egui::CentralPanel::default()
            .frame(Frame::none()
                .fill(AppColors::BACKGROUND)
                .inner_margin(16.0)
                .rounding(8.0))
            .show(ctx, |ui| {
                // Title with custom styling and spacing
                ui.add_space(8.0);
                ui.heading(
                    RichText::new("Sendme - Secure File Transfer")
                        .size(28.0)
                        .color(AppColors::TEXT_PRIMARY)
                );
                ui.add_space(24.0);  // More space after the title
                
                // Get states
                let is_running = *self.command_running.lock().unwrap();
                let is_ticket_ready = *self.is_ticket_ready.lock().unwrap();
                self.is_sending = is_running && is_ticket_ready;
                
                if self.is_sending && self.mode == AppMode::Receive {
                    self.mode = AppMode::Send;
                }
                
                // Mode selection tabs with modern styling
                ui.horizontal(|ui| {
                    ui.add_space(4.0);  // Small indent for tabs
                    
                    // Send tab
                    if ui.add(
                        egui::SelectableLabel::new(
                            self.mode == AppMode::Send,
                            RichText::new("📤 Send")
                                .size(16.0)
                                .color(if self.mode == AppMode::Send { 
                                    AppColors::PRIMARY 
                                } else { 
                                    AppColors::TEXT_SECONDARY 
                                })
                        )
                    ).clicked() {
                        self.mode = AppMode::Send;
                    }

                    ui.add_space(10.0);

                    // Receive tab
                    let receive_response = ui.add_enabled(
                        !self.is_sending,
                        egui::SelectableLabel::new(
                            self.mode == AppMode::Receive,
                            RichText::new("📥 Receive")
                                .size(16.0)
                                .color(if !self.is_sending && self.mode == AppMode::Receive { 
                                    AppColors::PRIMARY 
                                } else { 
                                    AppColors::DISABLED 
                                })
                        )
                    );

                    if receive_response.clicked() && !self.is_sending {
                        self.mode = AppMode::Receive;
                    }
                    
                    if self.is_sending {
                        receive_response.on_hover_text("Cannot switch to Receive mode while a sending session is active");
                    }
                });
                
                ui.add_space(20.0);
                
                match &self.mode {
                    AppMode::Send => {
                        // File selection section
                        ui.group(|ui| {
                            let frame = Frame::none()
                                .fill(AppColors::SURFACE)
                                .inner_margin(12.0)
                                .rounding(6.0);
                            frame.show(ui, |ui| {
                                ui.set_min_height(100.0);
                                ui.vertical(|ui| {
                                    ui.add_space(8.0);
                                    ui.heading(
                                        RichText::new("Select File or Directory")
                                            .size(18.0)
                                            .color(AppColors::TEXT_PRIMARY)
                                    );
                                    ui.add_space(12.0);
                                    
                                    ui.horizontal(|ui| {
                                        ui.add_space(4.0);  // Small indent for input field
                                        ui.add(
                                            TextEdit::singleline(&mut self.file_path)
                                                .desired_width(ui.available_width() - 120.0)
                                                .hint_text("Enter path or click Browse...")
                                                .text_color(AppColors::TEXT_PRIMARY)
                                                .frame(true)
                                                .margin(Vec2::new(8.0, 4.0))
                                        );
                                        
                                        let browse_response = ui.add_sized(
                                            [100.0, 30.0],
                                            egui::Button::new(
                                                RichText::new("Browse...")
                                                    .size(14.0)
                                                    .color(AppColors::TEXT_ON_COLOR)
                                            )
                                            .fill(if ui.rect_contains_pointer(ui.min_rect()) {
                                                AppColors::PRIMARY_LIGHT
                                            } else {
                                                AppColors::PRIMARY
                                            })
                                        );
                                        
                                        if browse_response.clicked() {
                                            if let Some(path) = rfd::FileDialog::new().pick_file() {
                                                self.file_path = path.display().to_string();
                                            }
                                        }
                                        
                                        browse_response.on_hover_text("Browse for a file or directory");
                                    });
                                    ui.add_space(8.0);  // Bottom padding for group
                                });
                            });
                        });
                        
                        ui.add_space(20.0);  // Space between major sections
                        
                        let is_ticket_ready = *self.is_ticket_ready.lock().unwrap();
                        
                        if is_ticket_ready {
                            ui.add_space(20.0);
                            // Ticket display section
                            ui.group(|ui| {
                                ui.set_min_height(120.0);
                                ui.vertical(|ui| {
                                    ui.add_space(5.0);
                                    ui.heading(
                                        RichText::new("🎟️ Your Transfer Ticket")
                                            .size(18.0)
                                            .color(AppColors::SUCCESS)
                                    );
                                    ui.label("Share this ticket with the receiver:");
                                    
                                    let ticket = self.extracted_ticket.lock().unwrap().clone();
                                    
                                    ui.add_space(5.0);
                                    ui.horizontal(|ui| {
                                        ui.add(
                                            TextEdit::multiline(&mut ticket.clone())
                                                .desired_width(ui.available_width() - 100.0)
                                                .desired_rows(1)
                                                .font(TextStyle::Monospace)
                                                .text_color(AppColors::TEXT_PRIMARY)
                                                .frame(true)
                                                .margin(Vec2::new(8.0, 4.0))
                                        );
                                        
                                        let copy_response = ui.add_sized(
                                            [80.0, 30.0],
                                            egui::Button::new(
                                                RichText::new("📋 Copy")
                                                    .size(14.0)
                                                    .color(AppColors::TEXT_ON_COLOR)
                                            )
                                            .fill(if ui.rect_contains_pointer(ui.min_rect()) {
                                                AppColors::PRIMARY_LIGHT
                                            } else {
                                                AppColors::PRIMARY
                                            })
                                        );
                                        
                                        if copy_response.clicked() {
                                            match Clipboard::new() {
                                                Ok(mut clipboard) => {
                                                    if clipboard.set_text(ticket.clone()).is_ok() {
                                                        self.status = "✅ Ticket copied to clipboard".to_string();
                                                    } else {
                                                        self.status = "❌ Failed to copy to clipboard".to_string();
                                                    }
                                                },
                                                Err(_) => {
                                                    self.status = "❌ Clipboard not available".to_string();
                                                }
                                            }
                                        }
                                        
                                        copy_response.on_hover_text("Copy the ticket to the clipboard");
                                    });
                                });
                            });
                        }
                        
                        ui.add_space(20.0);
                        
                        // Send button section
                        if !is_running {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let send_button = ui.add_sized(
                                    [120.0, 40.0],
                                    egui::Button::new(
                                        RichText::new("📤 Send File")
                                            .size(16.0)
                                            .color(AppColors::TEXT_ON_COLOR)
                                            .strong()
                                    )
                                    .fill(if !self.file_path.is_empty() {
                                        AppColors::SUCCESS
                                    } else {
                                        AppColors::DISABLED
                                    })
                                );
                                
                                if send_button.clicked() && !self.file_path.is_empty() {
                                    let path = PathBuf::from(&self.file_path);
                                    if !path.exists() {
                                        self.status = format!("❌ Error: Path '{}' does not exist", self.file_path);
                                    } else {
                                        self.status = format!("📤 Sending {}...", self.file_path);
                                        *self.command_running.lock().unwrap() = true;
                                        *self.is_ticket_ready.lock().unwrap() = false;
                                        
                                        // Clone all the necessary data before moving into the thread
                                        let output = self.output.clone();
                                        let extracted_ticket = self.extracted_ticket.clone();
                                        let is_ticket_ready = self.is_ticket_ready.clone();
                                        let path_clone = self.file_path.clone();
                                        let child_process = self.child_process.clone();
                                        let command_running = self.command_running.clone();
                                        
                                        *output.lock().unwrap() = String::new();
                                        *extracted_ticket.lock().unwrap() = String::new();
                                        
                                        std::thread::spawn(move || {
                                            let exe_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("sendme"));
                                            
                                            let mut child = Command::new(exe_path)
                                                .arg("send")
                                                .arg(path_clone)
                                                .stdout(std::process::Stdio::piped())
                                                .stderr(std::process::Stdio::piped())
                                                .spawn()
                                                .expect("Failed to start sendme process");
                                            
                                            let stdout = child.stdout.take();
                                            *child_process.lock().unwrap() = Some(child);
                                            
                                            if let Some(stdout) = stdout {
                                                let reader = std::io::BufReader::new(stdout);
                                                for line in reader.lines() {
                                                    if let Ok(line) = line {
                                                        let mut out = output.lock().unwrap();
                                                        *out = format!("{}\n{}", *out, line);
                                                        
                                                        if line.starts_with("sendme receive ") {
                                                            let ticket = line.trim_start_matches("sendme receive ").to_string();
                                                            *extracted_ticket.lock().unwrap() = ticket;
                                                            *is_ticket_ready.lock().unwrap() = true;
                                                        }
                                                    }
                                                }
                                            }
                                            
                                            *command_running.lock().unwrap() = false;
                                        });
                                    }
                                }
                                
                                if !self.file_path.is_empty() {
                                    send_button.on_hover_text("Click to start sending the file");
                                } else {
                                    send_button.on_hover_text("Please select a file first");
                                }
                            });
                        }
                        
                        // Status message and stop button
                        ui.add_space(8.0);  // Space before status message
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(&self.status)
                                    .size(14.0)
                                    .color(if self.status.contains("Error") {
                                        AppColors::DANGER
                                    } else if self.status.contains("✅") {
                                        AppColors::SUCCESS
                                    } else {
                                        AppColors::TEXT_PRIMARY
                                    })
                            );

                            // Add flexible space to push the stop button to the right
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if is_running {
                                    let stop_button = ui.add_sized(
                                        [80.0, 30.0],
                                        egui::Button::new(
                                            RichText::new("⏹ Stop")
                                                .size(14.0)
                                                .color(AppColors::TEXT_ON_COLOR)
                                                .strong()
                                        )
                                        .fill(AppColors::DANGER)
                                    );

                                    if stop_button.clicked() {
                                        if let Some(mut child) = self.child_process.lock().unwrap().take() {
                                            let _ = child.kill();
                                        }
                                        *self.command_running.lock().unwrap() = false;
                                        *self.is_ticket_ready.lock().unwrap() = false;
                                        self.status = "⏹ Transfer stopped".to_string();
                                    }

                                    stop_button.on_hover_text("Stop the current transfer");
                                }
                            });
                        });
                        ui.add_space(8.0);  // Bottom padding
                    }
                    
                    AppMode::Receive => {
                        // Receive section
                        ui.group(|ui| {
                            ui.set_min_height(100.0);
                            ui.vertical(|ui| {
                                ui.add_space(8.0);
                                ui.heading(
                                    RichText::new("Enter Transfer Ticket")
                                        .size(18.0)
                                        .color(AppColors::TEXT_PRIMARY)
                                );
                                ui.add_space(12.0);
                                
                                ui.horizontal(|ui| {
                                    ui.add(
                                        TextEdit::singleline(&mut self.ticket)
                                            .desired_width(ui.available_width() - 120.0)
                                            .hint_text("Paste the ticket here...")
                                            .text_color(AppColors::TEXT_PRIMARY)
                                            .frame(true)
                                            .margin(Vec2::new(8.0, 4.0))
                                    );
                                    
                                    let receive_response = ui.add_sized(
                                        [100.0, 30.0],
                                        egui::Button::new(
                                            RichText::new("📥 Receive")
                                                .size(14.0)
                                                .color(AppColors::TEXT_ON_COLOR)
                                                .strong()
                                        )
                                        .fill(if !self.ticket.is_empty() {
                                            if ui.rect_contains_pointer(ui.min_rect()) {
                                                AppColors::PRIMARY_LIGHT
                                            } else {
                                                AppColors::PRIMARY
                                            }
                                        } else {
                                            AppColors::DISABLED
                                        })
                                    );
                                    
                                    if receive_response.clicked() && !self.ticket.is_empty() {
                                        self.status = "📥 Receiving file...".to_string();
                                        *self.command_running.lock().unwrap() = true;
                                        
                                        let output = self.output.clone();
                                        let command_running = self.command_running.clone();
                                        let ticket = self.ticket.clone();
                                        
                                        std::thread::spawn(move || {
                                            let exe_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("sendme"));
                                            
                                            if let Ok(mut child) = Command::new(exe_path)
                                                .arg("receive")
                                                .arg(&ticket)
                                                .stdout(std::process::Stdio::piped())
                                                .stderr(std::process::Stdio::piped())
                                                .spawn()
                                            {
                                                if let Some(stdout) = child.stdout.take() {
                                                    let reader = std::io::BufReader::new(stdout);
                                                    for line in reader.lines() {
                                                        if let Ok(line) = line {
                                                            let mut out = output.lock().unwrap();
                                                            *out = format!("{}\n{}", *out, line);
                                                        }
                                                    }
                                                }
                                                
                                                let _ = child.wait();
                                            }
                                            
                                            *command_running.lock().unwrap() = false;
                                        });
                                    }
                                    
                                    if !self.ticket.is_empty() {
                                        receive_response.on_hover_text("Click to start receiving the file");
                                    } else {
                                        receive_response.on_hover_text("Please enter a ticket first");
                                    }
                                });
                            });
                        });
                        
                        // Output display
                        ui.add_space(20.0);
                        ScrollArea::vertical()
                            .max_height(200.0)
                            .show(ui, |ui| {
                                let output = self.output.lock().unwrap();
                                ui.add(
                                    TextEdit::multiline(&mut output.as_str())
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(10)
                                        .font(TextStyle::Monospace)
                                        .text_color(AppColors::TEXT_PRIMARY)
                                        .frame(true)
                                        .margin(Vec2::new(8.0, 4.0))
                                );
                            });
                    }
                }
            });
    }
}

impl Drop for SendmeApp {
    fn drop(&mut self) {
        // Terminate the process when the app is closed
        if let Some(mut child) = self.child_process.lock().unwrap().take() {
            // Try to kill the process gracefully
            if let Err(e) = child.kill() {
                eprintln!("Failed to kill process: {}", e);
            }
        }
    }
}

/// Run the GUI application
pub fn run_gui() -> Result<()> {
    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(800.0, 600.0)),
        min_window_size: Some(egui::vec2(600.0, 400.0)),
        resizable: true,
        default_theme: eframe::Theme::Light,
        ..Default::default()
    };

    eframe::run_native(
        "Sendme - Secure File Transfer",
        options,
        Box::new(|cc| Box::new(SendmeApp::default())),
    );

    Ok(())
}

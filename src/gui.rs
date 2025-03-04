use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::process::{Command, Child};
use anyhow::Result;
use eframe::{egui, App, CreationContext};
use egui::{TextEdit, ScrollArea, RichText, TextStyle};
use std::io::BufRead;
use arboard::Clipboard;

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
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Sendme - Secure File Transfer");
            
            // Get the command running state for tab enabling/disabling logic
            let is_running = *self.command_running.lock().unwrap();
            let is_ticket_ready = *self.is_ticket_ready.lock().unwrap();
            self.is_sending = is_running && is_ticket_ready;
            
            // If a sending session is active and user is somehow in Receive mode, force back to Send mode
            if self.is_sending && self.mode == AppMode::Receive {
                self.mode = AppMode::Send;
            }
            
            ui.horizontal(|ui| {
                // Always allow switching to Send tab
                ui.selectable_value(&mut self.mode, AppMode::Send, "Send");
                
                // Only allow switching to Receive tab if we're not in an active sending session
                if self.is_sending {
                    // Disabled receive tab when sending
                    let disabled_receive = ui.add_enabled(false, egui::SelectableLabel::new(
                        false, "Receive"
                    ));
                    disabled_receive.on_hover_text("Cannot switch to Receive mode while a sending session is active");
                } else {
                    // Normal selectable receive tab
                    let mut mode = self.mode;
                    ui.selectable_value(&mut mode, AppMode::Receive, "Receive");
                    if mode != self.mode {
                        self.mode = mode;
                        // Clear output when switching to Receive tab from Send tab
                        if mode == AppMode::Receive {
                            *self.output.lock().unwrap() = String::new();
                        }
                    }
                }
            });
            
            ui.add_space(10.0);
            
            match &self.mode {
                AppMode::Send => {
                    ui.horizontal(|ui| {
                        ui.label("File/Directory to send:");
                        let edit = ui.add(TextEdit::singleline(&mut self.file_path).desired_width(300.0));
                        if ui.button("Browse...").clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_file() {
                                self.file_path = path.display().to_string();
                            }
                        }
                        edit.on_hover_text("Enter the path to the file or directory you want to send");
                    });
                    
                    let is_ticket_ready = *self.is_ticket_ready.lock().unwrap();
                    
                    if is_ticket_ready {
                        // Display the extracted ticket in a copyable text box
                        ui.add_space(10.0);
                        ui.group(|ui| {
                            ui.heading(RichText::new("Ticket").strong());
                            ui.label("Share this ticket with the receiver (select and copy):");
                            
                            let ticket = self.extracted_ticket.lock().unwrap().clone();
                            
                            // Use a selectable text field with monospace font
                            ui.horizontal(|ui| {
                                ui.add(
                                    TextEdit::multiline(&mut ticket.clone())
                                        .desired_width(ui.available_width() - 80.0)
                                        .desired_rows(1)
                                        .font(TextStyle::Monospace)
                                );
                                
                                if ui.button("Copy").clicked() {
                                    match Clipboard::new() {
                                        Ok(mut clipboard) => {
                                            if clipboard.set_text(ticket.clone()).is_ok() {
                                                self.status = "Ticket copied to clipboard".to_string();
                                            } else {
                                                self.status = "Failed to copy to clipboard".to_string();
                                            }
                                        },
                                        Err(_) => {
                                            self.status = "Clipboard not available".to_string();
                                        }
                                    }
                                }
                            });
                            
                            ui.label("Keep this window open until the receiver has downloaded the file");
                        });
                    }
                    
                    if !is_running {
                        if ui.button("Send").clicked() && !self.file_path.is_empty() {
                            let path = PathBuf::from(&self.file_path);
                            if !path.exists() {
                                self.status = format!("Error: Path '{}' does not exist", self.file_path);
                            } else {
                                self.status = format!("Sending {}...", self.file_path);
                                *self.command_running.lock().unwrap() = true;
                                *self.is_ticket_ready.lock().unwrap() = false;
                                
                                let output = self.output.clone();
                                let _command_running = self.command_running.clone();
                                let extracted_ticket = self.extracted_ticket.clone();
                                let is_ticket_ready = self.is_ticket_ready.clone();
                                let path_clone = self.file_path.clone();
                                let child_process = self.child_process.clone();
                                
                                // Clear previous output
                                *output.lock().unwrap() = String::new();
                                *extracted_ticket.lock().unwrap() = String::new();
                                
                                // Run the CLI command as a child process
                                std::thread::spawn(move || {
                                    // Get the current executable path
                                    let exe_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("sendme"));
                                    
                                    // Start the process
                                    let mut child = Command::new(exe_path)
                                        .arg("send")
                                        .arg(path_clone)
                                        .stdout(std::process::Stdio::piped())
                                        .stderr(std::process::Stdio::piped())
                                        .spawn()
                                        .expect("Failed to start sendme process");
                                    
                                    // Get stdout before storing the child process
                                    let stdout = child.stdout.take();
                                    
                                    // Store the child process
                                    *child_process.lock().unwrap() = Some(child);
                                    
                                    // Read output in real-time
                                    if let Some(stdout) = stdout {
                                        let reader = std::io::BufReader::new(stdout);
                                        for line in reader.lines() {
                                            if let Ok(line) = line {
                                                // Add line to output
                                                let mut out = output.lock().unwrap();
                                                *out = format!("{}\n{}", *out, line);
                                                
                                                // Check if this line contains a ticket
                                                if line.starts_with("sendme receive ") {
                                                    let ticket = line.trim_start_matches("sendme receive ").to_string();
                                                    let mut extracted = extracted_ticket.lock().unwrap();
                                                    *extracted = ticket;
                                                    
                                                    // Set flag that ticket is ready
                                                    *is_ticket_ready.lock().unwrap() = true;
                                                }
                                            }
                                        }
                                    }
                                    
                                    // Keep the process running for send mode
                                    // We don't call child.wait() here because we want the process to continue
                                    // running in the background until the user closes the app
                                    
                                    // We keep command_running true, since the command should keep running
                                    // until the user closes the app or presses Ctrl+C in the terminal
                                });
                            }
                        }
                    } else if !is_ticket_ready {
                        ui.add_enabled(false, egui::Button::new("Send (in progress)"));
                        // Show a spinner or some indication that we're working
                        ui.spinner();
                    } else {
                        // If ticket is ready, show stop button
                        if ui.button("Stop Transfer").clicked() {
                            // Kill the process
                            if let Some(mut child) = self.child_process.lock().unwrap().take() {
                                if let Err(e) = child.kill() {
                                    self.status = format!("Failed to stop process: {}", e);
                                } else {
                                    self.status = "Transfer stopped".to_string();
                                    *self.command_running.lock().unwrap() = false;
                                    *self.is_ticket_ready.lock().unwrap() = false;
                                    *self.output.lock().unwrap() = String::new(); // Clear the output when stopping transfer
                                }
                            }
                        }
                    }
                },
                AppMode::Receive => {
                    ui.horizontal(|ui| {
                        ui.label("Ticket:");
                        ui.add(TextEdit::singleline(&mut self.ticket).desired_width(300.0))
                            .on_hover_text("Enter the ticket provided by the sender");
                    });
                    
                    // is_running was moved to the beginning of the update function
                    if !is_running {
                        if ui.button("Receive").clicked() && !self.ticket.is_empty() {
                            self.status = "Receiving file...".to_string();
                            *self.command_running.lock().unwrap() = true;
                            let output = self.output.clone();
                            let command_running = self.command_running.clone();
                            let ticket_clone = self.ticket.clone();
                            let child_process = self.child_process.clone();
                            
                            // Clear previous output
                            *output.lock().unwrap() = String::new();
                            
                            // Create a channel to communicate when the process is done
                            let (tx, rx) = std::sync::mpsc::channel();
                            
                            // Run the CLI command as a child process
                            std::thread::spawn(move || {
                                // Get the current executable path
                                let exe_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("sendme"));
                                
                                // Start the process
                                let mut child = Command::new(exe_path)
                                    .arg("receive")
                                    .arg(ticket_clone)
                                    .stdout(std::process::Stdio::piped())
                                    .stderr(std::process::Stdio::piped())
                                    .spawn()
                                    .expect("Failed to start sendme process");
                                
                                // Get stdout before storing the child process
                                let stdout = child.stdout.take();
                                
                                // Store the child process
                                *child_process.lock().unwrap() = Some(child);
                                
                                // Read output in real-time
                                if let Some(stdout) = stdout {
                                    let reader = std::io::BufReader::new(stdout);
                                    for line in reader.lines() {
                                        if let Ok(line) = line {
                                            let mut out = output.lock().unwrap();
                                            *out = format!("{}\n{}", *out, line);
                                        }
                                    }
                                }
                                
                                // Wait for the process to complete
                                let status = {
                                    let mut child_guard = child_process.lock().unwrap();
                                    if let Some(ref mut child) = *child_guard {
                                        child.wait().expect("Failed to wait for process")
                                    } else {
                                        // Process already gone somehow
                                        return;
                                    }
                                };
                                
                                // Update status and command_running flag
                                let mut out = output.lock().unwrap();
                                if status.success() {
                                    *out = format!("{}\n\nFile received successfully", *out);
                                } else {
                                    *out = format!("{}\n\nCommand failed with exit code: {:?}", *out, status.code());
                                }
                                
                                // Clean up the child process reference
                                *child_process.lock().unwrap() = None;
                                
                                // Signal that we're done
                                tx.send(()).unwrap();
                            });
                            
                            // Start a thread to wait for the signal and update command_running
                            let command_running_clone = command_running.clone();
                            std::thread::spawn(move || {
                                // Wait for the signal
                                let _ = rx.recv();
                                // Update the command_running flag
                                *command_running_clone.lock().unwrap() = false;
                            });
                        }
                    } else {
                        ui.add_enabled(false, egui::Button::new("Receive (in progress)"));
                        // Show a spinner or some indication that we're working
                        ui.spinner();
                        
                        // Add stop button while receiving
                        if ui.button("Stop Transfer").clicked() {
                            // Kill the process
                            if let Some(mut child) = self.child_process.lock().unwrap().take() {
                                if let Err(e) = child.kill() {
                                    self.status = format!("Failed to stop process: {}", e);
                                } else {
                                    self.status = "Transfer stopped".to_string();
                                    *self.command_running.lock().unwrap() = false;
                                }
                            }
                        }
                    }
                },
            }
            
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(5.0);
            
            ui.label(&self.status);
            
            ui.add_space(10.0);
            
            let output = self.output.lock().unwrap().clone();
            // Only show output console in receive mode when an actual receive operation is in progress
            let is_actively_receiving = self.mode == AppMode::Receive && !self.is_sending && is_running;
            
            if !output.is_empty() && is_actively_receiving {
                ui.group(|ui| {
                    ui.heading("Output");
                    ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                        ui.label(&output);
                    });
                });
            }
        });

        // Request repaint to update UI while command is running
        if *self.command_running.lock().unwrap() {
            ctx.request_repaint();
        }
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

pub fn run_gui() -> Result<()> {
    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(600.0, 400.0)),
        ..Default::default()
    };
    
    eframe::run_native(
        "Sendme - File Transfer",
        options,
        Box::new(|_cc: &CreationContext| Box::new(SendmeApp::default())),
    );
    
    Ok(())
}

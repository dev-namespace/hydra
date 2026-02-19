//! Skill setup functionality for Claude Code skills

use crate::config::Config;
use crate::error::{HydraError, Result};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{self, disable_raw_mode, enable_raw_mode};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

/// Embedded default skill templates
const DEV_SKILLS_TEMPLATE: &str = include_str!("../templates/skill-prompts/dev-skills.md");
const PERMISSIONS_TEMPLATE: &str = include_str!("../templates/skill-prompts/permissions.md");
const PRECOMMIT_TEMPLATE: &str = include_str!("../templates/skill-prompts/precommit.md");

/// Skill types that can be created
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SkillType {
    Permissions,
    DevSkills,
    Precommit,
}

impl SkillType {
    /// Get the skill name (used for directory and file naming)
    pub fn name(&self) -> &'static str {
        match self {
            SkillType::Permissions => "permissions",
            SkillType::DevSkills => "dev-skills",
            SkillType::Precommit => "precommit",
        }
    }

    /// Get the user-facing prompt text
    pub fn prompt_text(&self) -> &'static str {
        match self {
            SkillType::Permissions => "Configure Claude Code permissions?",
            SkillType::DevSkills => "Set up dev skills (local-dev-guide + deploy-and-check)?",
            SkillType::Precommit => "Set up precommit hooks?",
        }
    }

    /// Check if this is a permissions setup (not a skill)
    pub fn is_permissions(&self) -> bool {
        matches!(self, SkillType::Permissions)
    }

    /// Check if this is a precommit setup (not a skill)
    pub fn is_precommit(&self) -> bool {
        matches!(self, SkillType::Precommit)
    }

    /// Check if this is a dev skills setup (creates multiple skill directories)
    pub fn is_dev_skills(&self) -> bool {
        matches!(self, SkillType::DevSkills)
    }

    /// Get the embedded default template
    fn default_template(&self) -> &'static str {
        match self {
            SkillType::Permissions => PERMISSIONS_TEMPLATE,
            SkillType::DevSkills => DEV_SKILLS_TEMPLATE,
            SkillType::Precommit => PRECOMMIT_TEMPLATE,
        }
    }

    /// Get the path to the user-customizable template override
    fn override_template_path(&self) -> PathBuf {
        Config::global_skill_templates_dir().join(format!("{}.md", self.name()))
    }
}

/// Load the skill creation prompt template for a given skill type.
/// First checks for user override at ~/.hydra/skill-templates/<skill>.md,
/// then falls back to the embedded default.
pub fn load_skill_template(skill_type: SkillType) -> String {
    let override_path = skill_type.override_template_path();

    if override_path.exists() {
        // Try to read the override file
        match fs::read_to_string(&override_path) {
            Ok(content) => return content,
            Err(_) => {
                // Fall through to default if read fails
            }
        }
    }

    // Return the embedded default
    skill_type.default_template().to_string()
}

/// Prompt the user with a yes/no question that defaults to No.
///
/// Displays the prompt text followed by " [y/N] " and waits for user input.
/// - "y" or "Y" returns Ok(true)
/// - Empty input (just Enter), "n", or "N" returns Ok(false)
/// - Any other input is treated as No
///
/// # Arguments
/// * `prompt` - The question to display to the user
///
/// # Returns
/// * `Ok(true)` if user answers yes
/// * `Ok(false)` if user answers no or presses Enter
/// * `Err` if there's an I/O error reading input
pub fn prompt_yes_no(prompt: &str) -> Result<bool> {
    // Print prompt without newline
    print!("{} [y/N] ", prompt);
    io::stdout()
        .flush()
        .map_err(|e| HydraError::io("flushing stdout", e))?;

    // Read a line of input
    let stdin = io::stdin();
    let mut input = String::new();
    stdin
        .lock()
        .read_line(&mut input)
        .map_err(|e| HydraError::io("reading user input", e))?;

    // Trim and check the response
    let response = input.trim().to_lowercase();
    Ok(response == "y" || response == "yes")
}

/// Create a Claude Code skill by spawning Claude in interactive mode.
///
/// This function:
/// 1. Creates the `.claude/skills/<skill-name>/` directory structure (for skills, not permissions)
/// 2. Writes the skill creation prompt to a temporary file
/// 3. Spawns Claude via PTY in headful/interactive mode
/// 4. Waits for Claude to complete (user interacts with Claude)
///
/// # Arguments
/// * `skill_type` - The type of skill to create
/// * `verbose` - Whether to enable verbose output
///
/// # Returns
/// * `Ok(())` if Claude completes successfully
/// * `Err` if there's an error creating directories, writing the prompt, or spawning Claude
pub fn create_skill_with_claude(skill_type: SkillType, verbose: bool) -> Result<()> {
    let skill_name = skill_type.name();

    // For dev-skills, create both skill directories
    if skill_type.is_dev_skills() {
        for dir_name in &["local-dev-guide", "deploy-and-check"] {
            let skill_dir = PathBuf::from(".claude").join("skills").join(dir_name);
            if !skill_dir.exists() {
                fs::create_dir_all(&skill_dir).map_err(|e| {
                    HydraError::io(
                        format!("creating skill directory {}", skill_dir.display()),
                        e,
                    )
                })?;
                if verbose {
                    println!("Created {}", skill_dir.display());
                }
            }
        }
    }

    // Load the skill creation prompt template
    let prompt_content = load_skill_template(skill_type);

    // Write the prompt to a temporary file
    let temp_dir = std::env::temp_dir();
    let prompt_file = temp_dir.join(format!("hydra-skill-{}.md", skill_name));
    fs::write(&prompt_file, &prompt_content).map_err(|e| {
        HydraError::io(
            format!("writing skill prompt to {}", prompt_file.display()),
            e,
        )
    })?;

    if verbose {
        println!("Prompt written to: {}", prompt_file.display());
    }

    println!();
    if skill_type.is_permissions() {
        println!("─── Starting Claude to configure permissions ───");
    } else if skill_type.is_precommit() {
        println!("─── Starting Claude to set up precommit hooks ───");
    } else if skill_type.is_dev_skills() {
        println!("─── Starting Claude to create dev skills ───");
    } else {
        println!("─── Starting Claude to create {} skill ───", skill_name);
    }
    println!();

    // Spawn Claude in interactive mode via PTY
    let result = spawn_claude_interactive(&prompt_file, verbose);

    // Clean up the temporary prompt file
    let _ = fs::remove_file(&prompt_file);

    result
}

/// Messages sent from the PTY reader thread
enum SkillPtyMessage {
    Data(Vec<u8>),
    Closed,
    Error,
}

/// Spawn Claude in interactive (headful) mode and wait for it to complete.
///
/// This spawns Claude via PTY, forwards keyboard input, and displays output
/// until Claude exits.
fn spawn_claude_interactive(prompt_path: &PathBuf, verbose: bool) -> Result<()> {
    // Get terminal size
    let (cols, rows) = terminal::size().unwrap_or((80, 24));

    // Create PTY system
    let pty_system = native_pty_system();

    // Create PTY pair with current terminal size
    let pty_pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| HydraError::io("creating PTY pair", io::Error::other(e.to_string())))?;

    // Build command to run Claude (interactive mode, no --print flag)
    let mut cmd = CommandBuilder::new("claude");
    cmd.arg("--dangerously-skip-permissions");
    cmd.arg(prompt_path);

    // Set working directory to current directory
    let cwd =
        std::env::current_dir().map_err(|e| HydraError::io("getting current directory", e))?;
    cmd.cwd(cwd);

    // Spawn the command in the PTY
    let mut child = pty_pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| HydraError::io("spawning claude in PTY", io::Error::other(e.to_string())))?;

    // Get PTY reader and writer
    let pty_reader = pty_pair
        .master
        .try_clone_reader()
        .map_err(|e| HydraError::io("cloning PTY reader", io::Error::other(e.to_string())))?;
    let mut pty_writer = pty_pair
        .master
        .take_writer()
        .map_err(|e| HydraError::io("taking PTY writer", io::Error::other(e.to_string())))?;

    // Create channel for PTY reader thread to communicate back
    let (tx, rx): (Sender<SkillPtyMessage>, Receiver<SkillPtyMessage>) = mpsc::channel();

    // Spawn a thread to read from PTY (blocking read)
    let reader_handle = thread::spawn(move || {
        skill_pty_reader_thread(pty_reader, tx);
    });

    // Enable raw mode for stdin
    enable_raw_mode()
        .map_err(|e| HydraError::io("enabling raw mode", io::Error::other(e.to_string())))?;

    // Run the I/O loop
    let loop_result = skill_io_loop(&mut pty_writer, &rx, verbose);

    // Disable raw mode
    let _ = disable_raw_mode();

    // Restore terminal
    restore_terminal_for_skill();

    // Wait for child process to complete
    let _ = child.wait();

    // Drop PTY pair to close file descriptors
    drop(pty_pair);

    // Wait for reader thread (with timeout)
    let _ = reader_handle.join();

    loop_result
}

/// PTY reader thread - reads from PTY and sends data to main thread
fn skill_pty_reader_thread(mut reader: Box<dyn Read + Send>, tx: Sender<SkillPtyMessage>) {
    let mut buf = [0u8; 4096];

    loop {
        match reader.read(&mut buf) {
            Ok(0) => {
                // EOF - PTY closed
                let _ = tx.send(SkillPtyMessage::Closed);
                break;
            }
            Ok(n) => {
                if tx.send(SkillPtyMessage::Data(buf[..n].to_vec())).is_err() {
                    // Receiver dropped, exit
                    break;
                }
            }
            Err(_) => {
                let _ = tx.send(SkillPtyMessage::Error);
                break;
            }
        }
    }
}

/// Main I/O loop for skill creation - forward input to Claude, display output
fn skill_io_loop(
    pty_writer: &mut Box<dyn Write + Send>,
    rx: &Receiver<SkillPtyMessage>,
    _verbose: bool,
) -> Result<()> {
    // Write to /dev/tty instead of stdout to avoid potential issues
    let mut tty_output: Box<dyn Write> = match OpenOptions::new().write(true).open("/dev/tty") {
        Ok(tty) => Box::new(tty),
        Err(_) => Box::new(io::stdout()),
    };
    let poll_timeout = Duration::from_millis(10);

    loop {
        // Poll for keyboard input
        if event::poll(poll_timeout)
            .map_err(|e| HydraError::io("polling events", io::Error::other(e.to_string())))?
            && let Event::Key(key_event) = event::read()
                .map_err(|e| HydraError::io("reading event", io::Error::other(e.to_string())))?
        {
            // Forward keys to PTY
            let bytes = skill_key_event_to_bytes(&key_event);
            if !bytes.is_empty() {
                pty_writer
                    .write_all(&bytes)
                    .map_err(|e| HydraError::io("writing to PTY", e))?;
                pty_writer
                    .flush()
                    .map_err(|e| HydraError::io("flushing PTY", e))?;
            }
        }

        // Check for PTY output (non-blocking via try_recv)
        match rx.try_recv() {
            Ok(SkillPtyMessage::Data(data)) => {
                // Write to tty/stdout
                tty_output
                    .write_all(&data)
                    .map_err(|e| HydraError::io("writing to tty", e))?;
                tty_output
                    .flush()
                    .map_err(|e| HydraError::io("flushing tty", e))?;
            }
            Ok(SkillPtyMessage::Closed) => {
                // PTY closed - Claude exited
                return Ok(());
            }
            Ok(SkillPtyMessage::Error) => {
                // Error reading from PTY - Claude likely exited
                return Ok(());
            }
            Err(mpsc::TryRecvError::Empty) => {
                // No data available, continue
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                // Reader thread exited
                return Ok(());
            }
        }
    }
}

/// Convert a key event to bytes to send to PTY
fn skill_key_event_to_bytes(event: &crossterm::event::KeyEvent) -> Vec<u8> {
    let mut bytes = Vec::new();

    match event.code {
        KeyCode::Char(c) => {
            if event.modifiers.contains(KeyModifiers::CONTROL) {
                // Control characters
                if c.is_ascii_lowercase() {
                    bytes.push((c as u8) - b'a' + 1);
                } else if c.is_ascii_uppercase() {
                    bytes.push((c as u8) - b'A' + 1);
                }
            } else if event.modifiers.contains(KeyModifiers::ALT) {
                // Alt + char = ESC + char
                bytes.push(0x1b);
                bytes.extend(c.to_string().as_bytes());
            } else {
                bytes.extend(c.to_string().as_bytes());
            }
        }
        KeyCode::Enter => bytes.push(b'\r'),
        KeyCode::Tab => bytes.push(b'\t'),
        KeyCode::Backspace => bytes.push(0x7f),
        KeyCode::Esc => bytes.push(0x1b),
        KeyCode::Up => bytes.extend(b"\x1b[A"),
        KeyCode::Down => bytes.extend(b"\x1b[B"),
        KeyCode::Right => bytes.extend(b"\x1b[C"),
        KeyCode::Left => bytes.extend(b"\x1b[D"),
        KeyCode::Home => bytes.extend(b"\x1b[H"),
        KeyCode::End => bytes.extend(b"\x1b[F"),
        KeyCode::PageUp => bytes.extend(b"\x1b[5~"),
        KeyCode::PageDown => bytes.extend(b"\x1b[6~"),
        KeyCode::Insert => bytes.extend(b"\x1b[2~"),
        KeyCode::Delete => bytes.extend(b"\x1b[3~"),
        KeyCode::F(n) => {
            let seq = match n {
                1 => b"\x1bOP".to_vec(),
                2 => b"\x1bOQ".to_vec(),
                3 => b"\x1bOR".to_vec(),
                4 => b"\x1bOS".to_vec(),
                5 => b"\x1b[15~".to_vec(),
                6 => b"\x1b[17~".to_vec(),
                7 => b"\x1b[18~".to_vec(),
                8 => b"\x1b[19~".to_vec(),
                9 => b"\x1b[20~".to_vec(),
                10 => b"\x1b[21~".to_vec(),
                11 => b"\x1b[23~".to_vec(),
                12 => b"\x1b[24~".to_vec(),
                _ => vec![],
            };
            bytes.extend(seq);
        }
        _ => {}
    }

    bytes
}

/// Restore terminal to normal mode after skill creation
fn restore_terminal_for_skill() {
    // Comprehensive reset sequence
    let reset_sequence = concat!(
        "\x11",        // XON (Ctrl+Q) - resume if XOFF stopped terminal
        "\x18",        // CAN - cancel any partial escape sequence
        "\x1b[?2026l", // Disable synchronized output (used by Claude TUI)
        "\x1b[?1000l", // Disable mouse click tracking
        "\x1b[?1002l", // Disable mouse button tracking
        "\x1b[?1003l", // Disable mouse any-event tracking
        "\x1b[?1006l", // Disable SGR mouse mode
        "\x1b[?1015l", // Disable urxvt mouse mode
        "\x1b[?2004l", // Disable bracketed paste mode
        "\x1b[?1004l", // Disable focus reporting
        "\x1b[<u",     // Disable kitty keyboard protocol
        "\x1b[?1049l", // Exit alternate screen buffer
        "\x1b[?1l",    // Reset cursor keys mode
        "\x1b[?7h",    // Enable line wrapping
        "\x1b[?25h",   // Show cursor
        "\x1b[0m",     // Reset attributes
        "\x1b[r",      // Reset scroll region
    );

    // Try /dev/tty first (direct terminal access)
    #[cfg(unix)]
    {
        if let Ok(mut tty) = OpenOptions::new().write(true).open("/dev/tty") {
            let _ = tty.write_all(reset_sequence.as_bytes());
            let _ = tty.flush();
        } else {
            let mut stdout = io::stdout();
            let _ = stdout.write_all(reset_sequence.as_bytes());
            let _ = stdout.flush();
        }
    }

    #[cfg(not(unix))]
    {
        let mut stdout = io::stdout();
        let _ = stdout.write_all(reset_sequence.as_bytes());
        let _ = stdout.flush();
    }

    // Run stty sane as fallback
    #[cfg(unix)]
    {
        use std::process::Command;
        let _ = Command::new("stty")
            .arg("sane")
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_type_name() {
        assert_eq!(SkillType::Permissions.name(), "permissions");
        assert_eq!(SkillType::DevSkills.name(), "dev-skills");
        assert_eq!(SkillType::Precommit.name(), "precommit");
    }

    #[test]
    fn test_skill_type_prompt_text() {
        assert_eq!(
            SkillType::Permissions.prompt_text(),
            "Configure Claude Code permissions?"
        );
        assert_eq!(
            SkillType::DevSkills.prompt_text(),
            "Set up dev skills (local-dev-guide + deploy-and-check)?"
        );
        assert_eq!(
            SkillType::Precommit.prompt_text(),
            "Set up precommit hooks?"
        );
    }

    #[test]
    fn test_skill_type_is_permissions() {
        assert!(SkillType::Permissions.is_permissions());
        assert!(!SkillType::DevSkills.is_permissions());
        assert!(!SkillType::Precommit.is_permissions());
    }

    #[test]
    fn test_skill_type_is_precommit() {
        assert!(!SkillType::Permissions.is_precommit());
        assert!(!SkillType::DevSkills.is_precommit());
        assert!(SkillType::Precommit.is_precommit());
    }

    #[test]
    fn test_skill_type_is_dev_skills() {
        assert!(!SkillType::Permissions.is_dev_skills());
        assert!(SkillType::DevSkills.is_dev_skills());
        assert!(!SkillType::Precommit.is_dev_skills());
    }

    #[test]
    fn test_default_template_not_empty() {
        let permissions = SkillType::Permissions.default_template();
        let dev_skills = SkillType::DevSkills.default_template();
        let precommit = SkillType::Precommit.default_template();

        assert!(!permissions.is_empty());
        assert!(!dev_skills.is_empty());
        assert!(!precommit.is_empty());
        assert!(permissions.contains("settings.local.json"));
        assert!(dev_skills.contains("local-dev-guide"));
        assert!(dev_skills.contains("deploy-and-check"));
        assert!(precommit.contains("prek"));
    }

    #[test]
    fn test_load_skill_template_uses_default() {
        // When no override exists, should return the embedded default
        let template = load_skill_template(SkillType::DevSkills);
        assert!(template.contains("local-dev-guide"));
        assert!(template.contains("deploy-and-check"));
        assert!(template.contains("SKILL.md"));
    }

    #[test]
    fn test_override_template_path() {
        let path = SkillType::DevSkills.override_template_path();
        assert!(path.ends_with("dev-skills.md"));
        assert!(path.to_string_lossy().contains("skill-templates"));
    }
}

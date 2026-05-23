use std::io::Read;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use smartimu::{DeviceFrame, WireFrame, decode_binary_packet, decode_json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputMode {
    Auto,
    Json,
    Binary,
}

impl InputMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Json => "json",
            Self::Binary => "binary",
        }
    }
}

pub enum SerialEvent {
    Frame(DeviceFrame),
    Status(String),
    RawLine(String),
}

pub struct SerialConnection {
    receiver: mpsc::Receiver<SerialEvent>,
    #[cfg(windows)]
    powershell_child: Option<std::process::Child>,
}

impl SerialConnection {
    pub fn receiver(&self) -> &mpsc::Receiver<SerialEvent> {
        &self.receiver
    }
}

impl Drop for SerialConnection {
    fn drop(&mut self) {
        #[cfg(windows)]
        if let Some(mut child) = self.powershell_child.take() {
            let _ = child.kill();
        }
    }
}

pub fn available_ports() -> Vec<String> {
    serialport::available_ports()
        .map(|ports| ports.into_iter().map(|port| port.port_name).collect())
        .unwrap_or_default()
}

pub fn connect(port_name: String, baud_rate: u32, input_mode: InputMode) -> SerialConnection {
    let (tx, rx) = mpsc::channel();

    #[cfg(windows)]
    {
        cleanup_powershell_serial_readers(&port_name);
        if input_mode == InputMode::Json {
            match spawn_powershell_serial_reader(&port_name, baud_rate, tx.clone()) {
                Ok(child) => {
                    return SerialConnection {
                        receiver: rx,
                        powershell_child: Some(child),
                    };
                }
                Err(()) => {
                    let _ = tx.send(SerialEvent::Status(format!(
                        "failed to open {} via powershell",
                        port_name
                    )));
                }
            }
        }
    }

    let open_name = normalize_serial_port_name(&port_name);
    thread::spawn(move || {
        let port_result = serialport::new(open_name, baud_rate)
            .timeout(Duration::from_millis(200))
            .open();
        let Ok(mut port) = port_result else {
            let _ = tx.send(SerialEvent::Status(format!("failed to open {}", port_name)));
            return;
        };
        let _ = tx.send(SerialEvent::Status(format!("opened {}", port_name)));

        let mut chunk = [0u8; 256];
        let mut line = Vec::<u8>::new();
        let mut packet = Vec::<u8>::new();
        let mut detected = input_mode;
        let mut saw_frame = false;
        let mut idle_count = 0u32;

        loop {
            match port.read(&mut chunk) {
                Ok(0) => {
                    idle_count = idle_count.saturating_add(1);
                    if idle_count == 20 && !saw_frame {
                        let _ = tx.send(SerialEvent::Status(String::from(
                            "opened port, waiting for valid frames",
                        )));
                    }
                }
                Ok(read) => {
                    idle_count = 0;
                    for byte in &chunk[..read] {
                        match detected {
                            InputMode::Json => {
                                if *byte == b'\n' {
                                    if let Some(frame) = parse_json_line(&line) {
                                        saw_frame = true;
                                        let _ = tx
                                            .send(SerialEvent::Status(String::from("json stream")));
                                        if tx.send(SerialEvent::Frame(frame)).is_err() {
                                            return;
                                        }
                                    } else if let Some(line) = raw_text_line(&line) {
                                        let _ = tx.send(SerialEvent::RawLine(line));
                                    }
                                    line.clear();
                                } else {
                                    push_bounded(&mut line, *byte, 4096);
                                }
                            }
                            InputMode::Binary => {
                                if *byte == 0 {
                                    packet.push(0);
                                    if let Some(frame) = parse_binary_packet(&packet) {
                                        saw_frame = true;
                                        let _ = tx.send(SerialEvent::Status(String::from(
                                            "binary stream",
                                        )));
                                        if tx.send(SerialEvent::Frame(frame)).is_err() {
                                            return;
                                        }
                                    }
                                    packet.clear();
                                } else {
                                    push_bounded(&mut packet, *byte, 4096);
                                }
                            }
                            InputMode::Auto => {
                                if *byte == b'\n' {
                                    if let Some(frame) = parse_json_line(&line) {
                                        detected = InputMode::Json;
                                        saw_frame = true;
                                        let _ = tx.send(SerialEvent::Status(String::from(
                                            "auto -> json",
                                        )));
                                        if tx.send(SerialEvent::Frame(frame)).is_err() {
                                            return;
                                        }
                                    } else if let Some(line) = raw_text_line(&line) {
                                        let _ = tx.send(SerialEvent::RawLine(line));
                                    }
                                    line.clear();
                                } else if *byte == 0 {
                                    packet.push(0);
                                    if let Some(frame) = parse_binary_packet(&packet) {
                                        detected = InputMode::Binary;
                                        saw_frame = true;
                                        let _ = tx.send(SerialEvent::Status(String::from(
                                            "auto -> binary",
                                        )));
                                        if tx.send(SerialEvent::Frame(frame)).is_err() {
                                            return;
                                        }
                                    }
                                    packet.clear();
                                } else {
                                    push_bounded(&mut line, *byte, 4096);
                                    push_bounded(&mut packet, *byte, 4096);
                                }
                            }
                        }
                    }
                }
                Err(error) => {
                    let _ = tx.send(SerialEvent::Status(format!("serial read error: {}", error)));
                    thread::sleep(Duration::from_millis(20));
                }
            }
        }
    });

    SerialConnection {
        receiver: rx,
        #[cfg(windows)]
        powershell_child: None,
    }
}

#[cfg(windows)]
fn spawn_powershell_serial_reader(
    port_name: &str,
    baud_rate: u32,
    tx: mpsc::Sender<SerialEvent>,
) -> Result<std::process::Child, ()> {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};

    let script = format!(
        "$utf8 = New-Object System.Text.UTF8Encoding($false); \
         [Console]::OutputEncoding = $utf8; \
         $OutputEncoding = $utf8; \
         $port = New-Object System.IO.Ports.SerialPort '{port}',{baud},'None',8,'one'; \
         $port.ReadTimeout = 1000; \
         $port.Open(); \
         [Console]::WriteLine('__OPENED__'); \
         while ($true) {{ \
           try {{ \
             $line = $port.ReadLine(); \
             [Console]::WriteLine($line); \
           }} catch {{ Start-Sleep -Milliseconds 20 }} \
         }}",
        port = port_name,
        baud = baud_rate
    );

    let mut child = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-WindowStyle")
        .arg("Hidden")
        .arg("-Command")
        .arg(script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| ())?;

    let stdout = child.stdout.take().ok_or(())?;
    let stderr = child.stderr.take().ok_or(())?;
    let port_name = port_name.to_string();
    let tx_stderr = tx.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(line) if !line.trim().is_empty() => {
                    let _ = tx_stderr.send(SerialEvent::Status(format!(
                        "powershell stderr: {}",
                        line.trim()
                    )));
                }
                _ => {}
            }
        }
    });

    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let Ok(line) = line else {
                break;
            };
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed == "__OPENED__" {
                let _ = tx.send(SerialEvent::Status(format!(
                    "opened {} via powershell",
                    port_name
                )));
                continue;
            }
            if let Some(frame) = decode_json(trimmed).ok().and_then(wire_to_device) {
                let _ = tx.send(SerialEvent::Status(String::from(
                    "json stream (powershell)",
                )));
                if tx.send(SerialEvent::Frame(frame)).is_err() {
                    break;
                }
            } else {
                let _ = tx.send(SerialEvent::RawLine(trimmed.to_string()));
            }
        }
    });

    Ok(child)
}

#[cfg(windows)]
fn cleanup_powershell_serial_readers(port_name: &str) {
    use std::process::Command;

    let escaped = port_name.replace('\'', "''");
    let script = format!(
        "$port = '{port}'; \
         $portRegex = [Regex]::Escape($port); \
         Get-CimInstance Win32_Process | \
         Where-Object {{ \
           $_.Name -eq 'powershell.exe' -and \
           $_.CommandLine -match 'System\\.IO\\.Ports\\.SerialPort' -and \
           $_.CommandLine -match $portRegex \
         }} | \
         ForEach-Object {{ \
           try {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction Stop }} catch {{}} \
         }}",
        port = escaped
    );

    let _ = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-WindowStyle")
        .arg("Hidden")
        .arg("-Command")
        .arg(script)
        .status();
}

fn normalize_serial_port_name(port_name: &str) -> String {
    #[cfg(windows)]
    {
        let upper = port_name.to_ascii_uppercase();
        if upper.starts_with("COM") {
            let suffix = &port_name[3..];
            if suffix.parse::<u32>().map(|n| n >= 10).unwrap_or(false) {
                return format!(r"\\.\{}", port_name);
            }
        }
    }
    port_name.to_string()
}

fn parse_json_line(buffer: &[u8]) -> Option<DeviceFrame> {
    let line = std::str::from_utf8(buffer).ok()?.trim();
    if line.is_empty() {
        return None;
    }
    let json_start = line.find('{')?;
    decode_json(line[json_start..].trim())
        .ok()
        .and_then(wire_to_device)
}

fn parse_binary_packet(buffer: &[u8]) -> Option<DeviceFrame> {
    decode_binary_packet::<1024>(buffer)
        .ok()
        .and_then(wire_to_device)
}

fn wire_to_device(frame: WireFrame) -> Option<DeviceFrame> {
    match frame {
        WireFrame::Device(frame) => Some(frame),
        WireFrame::Host(_) => None,
    }
}

fn raw_text_line(buffer: &[u8]) -> Option<String> {
    let line = std::str::from_utf8(buffer).ok()?.trim();
    if line.is_empty() {
        return None;
    }
    Some(line.chars().take(240).collect())
}

fn push_bounded(buffer: &mut Vec<u8>, byte: u8, max: usize) {
    if buffer.len() >= max {
        buffer.clear();
    }
    buffer.push(byte);
}

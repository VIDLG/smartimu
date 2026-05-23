fn main() {
    let args: Vec<String> = std::env::args().collect();
    let port_name = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| String::from("COM15"));
    let baud_rate = args
        .get(2)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(115_200);

    let candidates = candidate_names(&port_name);
    println!("trying to open {} @ {}", port_name, baud_rate);

    for candidate in candidates {
        println!("candidate: {}", candidate);
        match serialport::new(&candidate, baud_rate)
            .timeout(std::time::Duration::from_millis(200))
            .open()
        {
            Ok(_) => {
                println!("OPEN OK: {}", candidate);
                return;
            }
            Err(error) => {
                println!("OPEN FAIL: {} => {}", candidate, error);
            }
        }
    }
}

fn candidate_names(port_name: &str) -> Vec<String> {
    let mut out = vec![port_name.to_string()];

    #[cfg(windows)]
    {
        let upper = port_name.to_ascii_uppercase();
        if upper.starts_with("COM") {
            let suffix = &port_name[3..];
            if suffix.parse::<u32>().map(|n| n >= 10).unwrap_or(false) {
                out.push(format!(r"\\.\{}", port_name));
            }
        }
    }

    out
}

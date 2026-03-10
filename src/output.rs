use serde::Serialize;

pub struct Output {
    json: bool,
}

impl Output {
    pub fn new(json: bool) -> Self {
        Self { json }
    }

    pub fn success(&self, message: &str) {
        if self.json {
            let v = serde_json::json!({ "status": "ok", "message": message });
            println!("{}", v);
        } else {
            println!("{}", message);
        }
    }

    pub fn error(&self, message: &str) {
        if self.json {
            let v = serde_json::json!({ "status": "error", "message": message });
            eprintln!("{}", v);
        } else {
            eprintln!("Error: {}", message);
        }
    }

    pub fn info(&self, key: &str, value: &str) {
        if self.json {
            // Accumulated by the caller via print_json
            return;
        }
        println!("{:<13}{}", format!("{}:", key), value);
    }

    pub fn print_json<T: Serialize>(&self, data: &T) {
        if self.json {
            if let Ok(json) = serde_json::to_string_pretty(data) {
                println!("{}", json);
            }
        }
    }

    pub fn is_json(&self) -> bool {
        self.json
    }

    pub fn println(&self, line: &str) {
        if !self.json {
            println!("{}", line);
        }
    }
}

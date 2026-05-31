use std::fs;
use std::path::Path;

pub fn upsert_config_section(config_path: &Path, section: &str, lines: &[String]) -> Result<(), String> {
    let parent = config_path.parent().ok_or("Invalid config path")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;

    let existing = if config_path.exists() {
        fs::read_to_string(config_path).map_err(|e| e.to_string())?
    } else {
        String::new()
    };

    let mut output = String::new();
    let mut in_section = false;
    let header = format!("[{section}]");

    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_section {
                in_section = false;
            }
            if trimmed == header {
                in_section = true;
                continue;
            }
        }
        if !in_section {
            output.push_str(line);
            output.push('\n');
        }
    }

    output.push_str(&header);
    output.push('\n');
    for line in lines {
        output.push_str(line);
        output.push('\n');
    }
    output.push('\n');

    fs::write(config_path, output).map_err(|e| e.to_string())
}

pub fn has_config_section(config_path: &Path, section: &str) -> bool {
    if !config_path.exists() {
        return false;
    }
    let Ok(content) = fs::read_to_string(config_path) else {
        return false;
    };
    let header = format!("[{section}]");
    content.lines().any(|line| line.trim() == header)
}

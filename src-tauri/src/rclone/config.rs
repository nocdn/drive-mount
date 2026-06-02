use std::fs;
use std::path::Path;

pub fn upsert_config_section(
    config_path: &Path,
    section: &str,
    lines: &[String],
) -> Result<(), String> {
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

pub fn read_config_section_lines(
    config_path: &Path,
    section: &str,
) -> Result<Option<Vec<String>>, String> {
    if !config_path.exists() {
        return Ok(None);
    }

    let existing = fs::read_to_string(config_path).map_err(|e| e.to_string())?;
    let header = format!("[{section}]");
    let mut in_section = false;
    let mut lines = Vec::new();

    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_section {
                break;
            }
            if trimmed == header {
                in_section = true;
            }
            continue;
        }
        if in_section {
            lines.push(line.to_string());
        }
    }

    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }

    if in_section {
        Ok(Some(lines))
    } else {
        Ok(None)
    }
}

pub fn remove_config_section(config_path: &Path, section: &str) -> Result<(), String> {
    if !config_path.exists() {
        return Ok(());
    }

    let existing = fs::read_to_string(config_path).map_err(|e| e.to_string())?;
    let header = format!("[{section}]");
    let mut output = String::new();
    let mut in_section = false;

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

    while output.ends_with("\n\n") {
        output.pop();
    }
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }

    if output.trim().is_empty() {
        fs::remove_file(config_path).map_err(|e| e.to_string())?;
    } else {
        fs::write(config_path, output).map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_config_section_creates_parent_and_new_section() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("nested").join("rclone.conf");

        upsert_config_section(
            &config,
            "b2remote",
            &[
                "type = b2".to_string(),
                "account = account-id".to_string(),
                "key = app-key".to_string(),
            ],
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(&config).unwrap(),
            "[b2remote]\ntype = b2\naccount = account-id\nkey = app-key\n\n"
        );
    }

    #[test]
    fn upsert_config_section_replaces_only_matching_section() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("rclone.conf");
        fs::write(
            &config,
            "[before]\ntype = local\n\n[gdrive]\ntype = drive\nold = yes\n\n[gdrive-extra]\ntype = alias\n\n[after]\ntype = sftp\n",
        )
        .unwrap();

        upsert_config_section(
            &config,
            "gdrive",
            &["type = drive".to_string(), "scope = drive".to_string()],
        )
        .unwrap();

        let content = fs::read_to_string(&config).unwrap();
        assert!(content.contains("[before]\ntype = local\n"));
        assert!(content.contains("[gdrive-extra]\ntype = alias\n"));
        assert!(content.contains("[after]\ntype = sftp\n"));
        assert!(content.ends_with("[gdrive]\ntype = drive\nscope = drive\n\n"));
        assert!(!content.contains("old = yes"));
    }

    #[test]
    fn has_config_section_matches_trimmed_header_only() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("rclone.conf");
        fs::write(
            &config,
            "  [gdrive]  \n[gdrive-extra]\n[seedbox]\ntype = ftp\n",
        )
        .unwrap();

        assert!(has_config_section(&config, "gdrive"));
        assert!(has_config_section(&config, "seedbox"));
        assert!(!has_config_section(&config, "drive"));
        assert!(!has_config_section(&config, "missing"));
    }

    #[test]
    fn read_config_section_lines_returns_only_target_body() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("rclone.conf");
        fs::write(
            &config,
            "[before]\ntype = local\n\n[gdrive]\ntype = drive\nscope = drive\ntoken = {}\n\n[after]\ntype = sftp\n",
        )
        .unwrap();

        assert_eq!(
            read_config_section_lines(&config, "gdrive").unwrap(),
            Some(vec![
                "type = drive".to_string(),
                "scope = drive".to_string(),
                "token = {}".to_string(),
            ])
        );
        assert_eq!(read_config_section_lines(&config, "missing").unwrap(), None);
    }

    #[test]
    fn remove_config_section_deletes_only_target_section() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("rclone.conf");
        fs::write(
            &config,
            "[before]\ntype = local\n\n[gdrive]\ntype = drive\nold = yes\n\n[after]\ntype = sftp\n",
        )
        .unwrap();

        remove_config_section(&config, "gdrive").unwrap();

        let content = fs::read_to_string(&config).unwrap();
        assert!(content.contains("[before]\ntype = local\n"));
        assert!(content.contains("[after]\ntype = sftp\n"));
        assert!(!content.contains("[gdrive]"));
        assert!(!content.contains("old = yes"));
        assert!(content.ends_with('\n'));
    }

    #[test]
    fn remove_config_section_removes_file_when_last_section_is_deleted() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("rclone.conf");
        fs::write(&config, "[seedbox]\ntype = ftp\n").unwrap();

        remove_config_section(&config, "seedbox").unwrap();

        assert!(!config.exists());
    }

    #[test]
    fn remove_config_section_is_noop_for_missing_file_or_section() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("missing.conf");
        remove_config_section(&missing, "gdrive").unwrap();
        assert!(!missing.exists());

        let config = temp.path().join("rclone.conf");
        fs::write(&config, "[seedbox]\ntype = ftp\n").unwrap();
        remove_config_section(&config, "gdrive").unwrap();
        assert_eq!(
            fs::read_to_string(&config).unwrap(),
            "[seedbox]\ntype = ftp\n"
        );
    }
}

use std::{fs, path::Path};

use anyhow::Result;

const DATABASE_STATE_PATH: &str = "runtime/database-state.json";

pub fn write_json(summary_json: &str) -> Result<()> {
    write_json_at(Path::new(DATABASE_STATE_PATH), summary_json)
}

fn write_json_at(path: &Path, summary_json: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, summary_json)?;
    fs::rename(tmp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_json_creates_runtime_parent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state_path = temp_dir.path().join("runtime/database-state.json");

        write_json_at(&state_path, "{\n  \"status\": \"success\"\n}").unwrap();

        assert_eq!(
            std::fs::read_to_string(state_path).unwrap(),
            "{\n  \"status\": \"success\"\n}"
        );
    }

    #[test]
    fn write_json_replaces_previous_snapshot() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state_path = temp_dir.path().join("runtime/database-state.json");

        write_json_at(&state_path, "{\"status\":\"old\"}").unwrap();
        write_json_at(&state_path, "{\"status\":\"success\"}").unwrap();

        assert_eq!(
            std::fs::read_to_string(state_path).unwrap(),
            "{\"status\":\"success\"}"
        );
    }
}

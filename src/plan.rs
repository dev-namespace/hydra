use crate::error::{HydraError, Result};
use std::fs;
use std::path::PathBuf;

/// Read plan file content from the given path
///
/// Returns the file content if it exists, or a PlanNotFound error if the file doesn't exist.
pub fn read_plan_file(path: &PathBuf) -> Result<String> {
    if !path.exists() {
        return Err(HydraError::PlanNotFound(path.clone()));
    }

    fs::read_to_string(path)
        .map_err(|e| HydraError::io(format!("reading plan file {}", path.display()), e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_read_plan_file_success() {
        let temp_dir = TempDir::new().unwrap();
        let plan_path = temp_dir.path().join("plan.md");
        let plan_content = "## Tasks\n\n- [ ] Task 1\n- [ ] Task 2\n";
        fs::write(&plan_path, plan_content).unwrap();

        let result = read_plan_file(&plan_path).unwrap();
        assert_eq!(result, plan_content);
    }

    #[test]
    fn test_read_plan_file_not_found() {
        let nonexistent = PathBuf::from("/nonexistent/path/plan.md");
        let result = read_plan_file(&nonexistent);

        assert!(result.is_err());
        match result.unwrap_err() {
            HydraError::PlanNotFound(path) => {
                assert_eq!(path, nonexistent);
            }
            e => panic!("Expected PlanNotFound error, got: {:?}", e),
        }
    }

    #[test]
    fn test_read_plan_file_error_message() {
        let nonexistent = PathBuf::from("/some/path/implementation.md");
        let err = read_plan_file(&nonexistent).unwrap_err();

        let error_message = err.to_string();
        assert!(
            error_message.contains("/some/path/implementation.md"),
            "Error message should contain the path: {}",
            error_message
        );
    }
}

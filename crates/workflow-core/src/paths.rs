/// Path constants and utilities for the workflow system
use std::path::PathBuf;
use once_cell::sync::OnceCell;

// Static storage for configurable data root
static DATA_ROOT: OnceCell<String> = OnceCell::new();

// Static storage for configurable logs root
static LOGS_ROOT: OnceCell<String> = OnceCell::new();

// Static storage for configurable templates root
static TEMPLATES_ROOT: OnceCell<String> = OnceCell::new();

// Default root constants
const DEFAULT_WORKFLOW_DATA_ROOT: &str = "/data/workflows";
const DEFAULT_LOGS_ROOT: &str = "/logs";
const DEFAULT_TEMPLATES_ROOT: &str = "/app/templates";
pub const APP_ROOT: &str = "/app";

/// Initialize the data root directory. Can only be called once.
/// If not called, the default `/data/workflows` will be used.
pub fn init_data_root(path: String) -> Result<(), String> {
    DATA_ROOT.set(path).map_err(|_| "Data root already initialized".to_string())
}

/// Initialize the logs root directory. Can only be called once.
/// If not called, the default `/logs` will be used.
pub fn init_logs_root(path: String) -> Result<(), String> {
    LOGS_ROOT.set(path).map_err(|_| "Logs root already initialized".to_string())
}

/// Initialize the templates root directory. Can only be called once.
/// If not called, the default `/app/templates` will be used.
pub fn init_templates_root(path: String) -> Result<(), String> {
    TEMPLATES_ROOT.set(path).map_err(|_| "Templates root already initialized".to_string())
}

/// Get the configured data root or the default
fn get_data_root() -> &'static str {
    DATA_ROOT.get().map(|s| s.as_str()).unwrap_or(DEFAULT_WORKFLOW_DATA_ROOT)
}

/// Get the configured logs root or the default
fn get_logs_root() -> &'static str {
    LOGS_ROOT.get().map(|s| s.as_str()).unwrap_or(DEFAULT_LOGS_ROOT)
}

/// Get the configured templates root or the default
fn get_templates_root() -> &'static str {
    TEMPLATES_ROOT.get().map(|s| s.as_str()).unwrap_or(DEFAULT_TEMPLATES_ROOT)
}

// Get the workflow data root directory
pub fn workflow_data_root_str() -> &'static str {
    get_data_root()
}

// Directory names (relative to roots)
pub const TRIGGERS_DIR_NAME: &str = "triggers";
pub const PROCESSED_DIR_NAME: &str = "processed";
pub const FAILED_DIR_NAME: &str = "failed";
pub const DATA_DIR_NAME: &str = "data";

// Approval state directories
pub const PENDING_APPROVAL_DIR_NAME: &str = "pending_approval";
pub const AWAITING_RESPONSE_DIR_NAME: &str = "awaiting_response";
pub const APPROVED_DIR_NAME: &str = "approved";
pub const NEEDS_IMPROVEMENT_DIR_NAME: &str = "needs_improvement";
pub const FAILED_STATE_DIR_NAME: &str = "failed";

// Data subdirectories
pub const DOSSIERS_DIR_NAME: &str = "dossiers";
pub const LETTERS_DIR_NAME: &str = "letters";
pub const ATTACHMENTS_DIR_NAME: &str = "attachments";

// App subdirectories
pub const CONFIG_DIR_NAME: &str = "config";
pub const TEMPLATES_DIR_NAME: &str = "templates";

// Path builder functions
pub fn workflow_data_root() -> PathBuf {
    PathBuf::from(get_data_root())
}

pub fn triggers_dir() -> PathBuf {
    workflow_data_root().join(TRIGGERS_DIR_NAME)
}

pub fn triggers_processed_dir() -> PathBuf {
    triggers_dir().join(PROCESSED_DIR_NAME)
}

pub fn triggers_failed_dir() -> PathBuf {
    triggers_dir().join(FAILED_DIR_NAME)
}

pub fn data_dir() -> PathBuf {
    workflow_data_root().join(DATA_DIR_NAME)
}

pub fn dossiers_dir() -> PathBuf {
    data_dir().join(DOSSIERS_DIR_NAME)
}

pub fn letters_dir() -> PathBuf {
    data_dir().join(LETTERS_DIR_NAME)
}

pub fn attachments_dir() -> PathBuf {
    data_dir().join(ATTACHMENTS_DIR_NAME)
}

pub fn approval_state_dir(state_name: &str) -> PathBuf {
    workflow_data_root().join(state_name)
}

pub fn pending_approval_dir() -> PathBuf {
    approval_state_dir(PENDING_APPROVAL_DIR_NAME)
}

pub fn awaiting_response_dir() -> PathBuf {
    approval_state_dir(AWAITING_RESPONSE_DIR_NAME)
}

pub fn approved_dir() -> PathBuf {
    approval_state_dir(APPROVED_DIR_NAME)
}

pub fn needs_improvement_dir() -> PathBuf {
    approval_state_dir(NEEDS_IMPROVEMENT_DIR_NAME)
}

pub fn failed_state_dir() -> PathBuf {
    approval_state_dir(FAILED_STATE_DIR_NAME)
}

pub fn app_root() -> PathBuf {
    PathBuf::from(APP_ROOT)
}

pub fn config_dir() -> PathBuf {
    app_root().join(CONFIG_DIR_NAME)
}

pub fn credentials_path() -> PathBuf {
    config_dir().join("credentials.json")
}

pub fn templates_dir() -> PathBuf {
    PathBuf::from(get_templates_root())
}

// Logs directory functions
pub fn logs_root() -> PathBuf {
    PathBuf::from(get_logs_root())
}

pub fn grpc_logs_dir() -> PathBuf {
    logs_root().join("grpc")
}

pub fn dossier_logs_dir() -> PathBuf {
    grpc_logs_dir().join("dossier")
}

/// Get all directories that should be created for the workflow system
pub fn all_workflow_directories() -> Vec<PathBuf> {
    vec![
        workflow_data_root(),
        triggers_dir(),
        triggers_processed_dir(),
        triggers_failed_dir(),
        data_dir(),
        dossiers_dir(),
        letters_dir(),
        attachments_dir(),
        pending_approval_dir(),
        awaiting_response_dir(),
        approved_dir(),
        needs_improvement_dir(),
        failed_state_dir(),
    ]
}

// Tests module
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_root_constants() {
        assert_eq!(workflow_data_root_str(), "/data/workflows");
        assert_eq!(APP_ROOT, "/app");
    }

    #[test]
    fn test_path_building_from_root() {
        // Test that all paths are built from the root constants
        assert_eq!(triggers_dir().to_str().unwrap(), "/data/workflows/triggers");
        assert_eq!(triggers_processed_dir().to_str().unwrap(), "/data/workflows/triggers/processed");
        assert_eq!(triggers_failed_dir().to_str().unwrap(), "/data/workflows/triggers/failed");
        
        assert_eq!(data_dir().to_str().unwrap(), "/data/workflows/data");
        assert_eq!(dossiers_dir().to_str().unwrap(), "/data/workflows/data/dossiers");
        assert_eq!(letters_dir().to_str().unwrap(), "/data/workflows/data/letters");
        assert_eq!(attachments_dir().to_str().unwrap(), "/data/workflows/data/attachments");
    }

    #[test]
    fn test_approval_state_directories() {
        assert_eq!(pending_approval_dir().to_str().unwrap(), "/data/workflows/pending_approval");
        assert_eq!(awaiting_response_dir().to_str().unwrap(), "/data/workflows/awaiting_response");
        assert_eq!(approved_dir().to_str().unwrap(), "/data/workflows/approved");
        assert_eq!(needs_improvement_dir().to_str().unwrap(), "/data/workflows/needs_improvement");
        assert_eq!(failed_state_dir().to_str().unwrap(), "/data/workflows/failed");
    }

    #[test]
    fn test_app_paths() {
        assert_eq!(config_dir().to_str().unwrap(), "/app/config");
        assert_eq!(credentials_path().to_str().unwrap(), "/app/config/credentials.json");
        // Templates dir now uses configurable path with default
        assert_eq!(templates_dir().to_str().unwrap(), DEFAULT_TEMPLATES_ROOT);
    }

    #[test]
    fn test_all_paths_start_with_roots() {
        // Verify that all workflow paths start with the workflow root
        let workflow_paths = vec![
            triggers_dir(),
            triggers_processed_dir(),
            triggers_failed_dir(),
            data_dir(),
            dossiers_dir(),
            letters_dir(),
            attachments_dir(),
            pending_approval_dir(),
            awaiting_response_dir(),
            approved_dir(),
            needs_improvement_dir(),
            failed_state_dir(),
        ];

        for path in workflow_paths {
            assert!(
                path.starts_with(get_data_root()),
                "Path {:?} should start with WORKFLOW_DATA_ROOT",
                path
            );
        }

        // Verify app paths start with app root (except templates which is now configurable)
        let app_paths = vec![
            config_dir(),
            credentials_path(),
        ];

        for path in app_paths {
            assert!(
                path.starts_with(APP_ROOT),
                "Path {:?} should start with APP_ROOT",
                path
            );
        }
        
        // Templates dir uses its own configurable root
        assert!(
            templates_dir().starts_with(get_templates_root()),
            "Templates dir should start with templates root"
        );
    }

    #[test]
    fn test_all_directories_unique() {
        let all_dirs = all_workflow_directories();
        let unique_dirs: HashSet<_> = all_dirs.iter().collect();
        
        assert_eq!(
            all_dirs.len(),
            unique_dirs.len(),
            "All directories should be unique"
        );
    }

    #[test]
    fn test_no_hardcoded_paths_in_functions() {
        // This test verifies that functions build paths dynamically
        // If we change WORKFLOW_DATA_ROOT, all dependent paths should change
        
        let triggers_path = triggers_dir();
        assert!(triggers_path.to_str().unwrap().contains(get_data_root()));
        assert!(triggers_path.to_str().unwrap().contains(TRIGGERS_DIR_NAME));
        
        let processed_path = triggers_processed_dir();
        assert!(processed_path.to_str().unwrap().contains(get_data_root()));
        assert!(processed_path.to_str().unwrap().contains(TRIGGERS_DIR_NAME));
        assert!(processed_path.to_str().unwrap().contains(PROCESSED_DIR_NAME));
    }

    #[test]
    fn test_directory_hierarchy() {
        // Test that subdirectories are properly nested
        assert!(triggers_processed_dir().starts_with(triggers_dir()));
        assert!(triggers_failed_dir().starts_with(triggers_dir()));
        
        assert!(dossiers_dir().starts_with(data_dir()));
        assert!(letters_dir().starts_with(data_dir()));
        assert!(attachments_dir().starts_with(data_dir()));
        
        assert!(credentials_path().starts_with(config_dir()));
    }

    #[test]
    fn test_all_workflow_directories_coverage() {
        let all_dirs = all_workflow_directories();
        
        // Verify all expected directories are included
        assert!(all_dirs.contains(&workflow_data_root()));
        assert!(all_dirs.contains(&triggers_dir()));
        assert!(all_dirs.contains(&triggers_processed_dir()));
        assert!(all_dirs.contains(&triggers_failed_dir()));
        assert!(all_dirs.contains(&data_dir()));
        assert!(all_dirs.contains(&dossiers_dir()));
        assert!(all_dirs.contains(&letters_dir()));
        assert!(all_dirs.contains(&attachments_dir()));
        assert!(all_dirs.contains(&pending_approval_dir()));
        assert!(all_dirs.contains(&awaiting_response_dir()));
        assert!(all_dirs.contains(&approved_dir()));
        assert!(all_dirs.contains(&needs_improvement_dir()));
        assert!(all_dirs.contains(&failed_state_dir()));
        
        // Should have exactly 13 directories
        assert_eq!(all_dirs.len(), 13);
    }

    #[test]
    fn test_path_consistency_with_approval_states() {
        // Test that approval state names match what ApprovalState enum would use
        let expected_state_dirs = vec![
            "pending_approval",
            "awaiting_response", 
            "approved",
            "needs_improvement",
            "failed",
        ];

        for state_name in expected_state_dirs {
            let state_dir = approval_state_dir(state_name);
            assert_eq!(
                state_dir.to_str().unwrap(),
                format!("{}/{}", get_data_root(), state_name)
            );
        }
    }
}
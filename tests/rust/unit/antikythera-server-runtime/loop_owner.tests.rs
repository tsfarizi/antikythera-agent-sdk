use super::*;

#[test]
fn default_config_disables_runtime_hooks() {
    let config = ToolLoopConfig::default();
    assert!(!config.runtime_hooks_enabled);
    assert!(!config.force_json);
    assert_eq!(config.max_steps, 10);
}

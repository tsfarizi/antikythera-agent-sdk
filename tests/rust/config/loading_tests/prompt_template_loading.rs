#[test]
/// TomlAppConfig::default() leaves template as the built-in default string.
///
fn uses_default_template_when_prompts_field_is_empty() {
    let dir = tempdir().expect("tempdir");
    let pc = minimal_toml_config();
    let path = write_toml_config(dir.path(), &pc);

    let config = AppConfig::load(Some(&path)).expect("load config");
    assert!(
        !config.prompt_template().is_empty(),
        "default template should not be empty"
    );
}

#[test]
fn loads_custom_prompt_template_from_toml() {
    let dir = tempdir().expect("tempdir");
    let mut pc = minimal_toml_config();
    pc.prompts.template = "Custom system prompt.".to_string();
    let path = write_toml_config(dir.path(), &pc);

    let config = AppConfig::load(Some(&path)).expect("load config");
    assert_eq!(config.prompt_template(), "Custom system prompt.");
}

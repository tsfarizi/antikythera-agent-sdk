#[test]
fn loads_toml_config_roundtrip_preserves_routing() {
    let dir = tempdir().expect("tempdir");
    let mut pc = minimal_toml_config();
    pc.model.default_provider = "ollama".to_string();
    pc.model.model = "llama3".to_string();
    let path = write_toml_config(dir.path(), &pc);

    let config = AppConfig::load(Some(&path)).expect("load config");
    assert_eq!(config.default_provider(), "ollama");
    assert_eq!(config.model_name(), "llama3");
}

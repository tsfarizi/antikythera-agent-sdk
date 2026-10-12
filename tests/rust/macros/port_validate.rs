use antikythera_macros::PortValidate;

/// Port trait that an implementation must satisfy.
trait ModelClient: Send + Sync {
    fn id(&self) -> &str;
    fn chat(&self, request: &str) -> String;
}

/// Valid implementation that satisfies all trait requirements.
#[derive(PortValidate)]
#[implements(ModelClient)]
struct ValidClient {
    client_id: String,
}

impl ModelClient for ValidClient {
    fn id(&self) -> &str {
        &self.client_id
    }

    fn chat(&self, request: &str) -> String {
        format!("Response to: {}", request)
    }
}

#[test]
fn test_valid_implementation_compiles() {
    let client = ValidClient {
        client_id: "test".to_string(),
    };
    assert_eq!(client.id(), "test");
    assert_eq!(client.chat("hello"), "Response to: hello");
}

/// Trait with no required methods.
#[allow(dead_code)]
trait MarkerTrait: Send + Sync {}

#[derive(PortValidate)]
#[implements(MarkerTrait)]
struct MarkerImpl;

impl MarkerTrait for MarkerImpl {}

#[test]
fn test_marker_trait_implementation() {
    let _ = MarkerImpl;
}

/// Trait with associated type.
trait Repository {
    type Item;
    fn get(&self, id: &str) -> Option<Self::Item>;
}

#[derive(PortValidate)]
#[implements(Repository)]
struct InMemoryRepo {
    items: Vec<String>,
}

impl Repository for InMemoryRepo {
    type Item = String;

    fn get(&self, id: &str) -> Option<Self::Item> {
        self.items.iter().find(|i| i.as_str() == id).cloned()
    }
}

#[test]
fn test_associated_type_implementation() {
    let repo = InMemoryRepo {
        items: vec!["a".to_string(), "b".to_string()],
    };
    assert_eq!(repo.get("a"), Some("a".to_string()));
    assert_eq!(repo.get("c"), None);
}

/// Trait in a nested module path.
mod traits {
    pub trait Storage {
        fn store(&self, key: &str, value: &[u8]);
    }
}

#[derive(PortValidate)]
#[implements(traits::Storage)]
struct FileStorage;

impl traits::Storage for FileStorage {
    fn store(&self, _key: &str, _value: &[u8]) {}
}

#[test]
fn test_nested_path_trait_implementation() {
    use traits::Storage;
    let storage = FileStorage;
    storage.store("key", b"value");
}

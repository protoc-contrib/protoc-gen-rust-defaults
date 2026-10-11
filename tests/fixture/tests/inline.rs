//! Defaults included into buffa's own module for their package
//! (`packaging=false`), the way grpc-rust-template mounts them.

use buffa::EnumValue;
use defaults_fixture::inline::v1::{ListBooksRequest, Shelf, shelf};

#[test]
fn defaults_a_page_size() {
    let mut request = ListBooksRequest::default();
    request.set_defaults();
    assert_eq!(request.page_size, 50);

    let mut request = ListBooksRequest {
        page_size: 10,
        ..Default::default()
    };
    request.set_defaults();
    assert_eq!(request.page_size, 10);
}

#[test]
fn resolves_nested_types_from_buffas_module() {
    let mut message = Shelf::default();
    message.set_defaults();
    assert_eq!(message.kind, EnumValue::Known(shelf::Kind::KIND_TALL));
    assert_eq!(message.request.page_size, 50);
    assert_eq!(
        message.placement,
        Some(shelf::Placement::Wall("north".to_owned()))
    );
}

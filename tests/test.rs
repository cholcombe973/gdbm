extern crate gdbm;
extern crate libc;

use std::path::Path;

use libc::{S_IRUSR, S_IWUSR};

#[test]
fn create_test() {
    // Should create a dbm
    let result = gdbm::Gdbm::new(Path::new("test"),
                                 0,
                                 gdbm::Open::NEWDB,
                                 (S_IRUSR | S_IWUSR) as i32)
        .unwrap();
    // Lets write a key/value and then read it back
    let mut data = "blah".to_string();
    let store_result = result.store("foo", &mut data, gdbm::Store::INSERT);
    assert_eq!(store_result, 0);
    let fetch_result = result.fetch("foo").unwrap();
    assert_eq!("blah".to_string(), fetch_result);
}

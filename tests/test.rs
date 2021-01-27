extern crate gdbm;
extern crate libc;

use std::path::Path;
use std::fs::remove_file;

use libc::{S_IRUSR, S_IWUSR};

#[test]
fn create_test() {
    // Should create a dbm
    let _  = remove_file("test.db");
    let db = gdbm::Gdbm::new(Path::new("test.db"),
                                 0,
                                 gdbm::Open::NEWDB,
                                 (S_IRUSR | S_IWUSR) as i32)
        .expect("Gdbm::new");
    // Lets write a key/value and then read it back
    let data = "blah".to_string();
    let store_result = db.store("foo", &data, true).expect("store");
    assert_eq!(store_result, true);
    let store_result = db.store("foo", &data, false).expect("store");
    assert_eq!(store_result, false);
    let fetch_result = db.fetch("foo").expect("fetch");
    assert_eq!("blah".to_string(), fetch_result);
    drop(db);
    remove_file("test.db").expect("remove_file");
}

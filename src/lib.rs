#[macro_use]
extern crate bitflags;
use anyhow::anyhow;
use thiserror::Error;

use std::{
    ffi::{CStr, CString, NulError},
    fmt,
    io::Error,
    os::unix::ffi::OsStrExt,
    path::Path,
    str::Utf8Error,
};

use libc::{c_uint, c_void, free};

use gdbm_sys::*;

/// Custom error handling for the library
#[derive(Debug, Error)]
pub enum GdbmError {
    Utf8Error {
        #[from]
        source: Utf8Error,
    },
    NulError {
        #[from]
        source: NulError,
    },
    Error {
        #[from]
        source: Error,
    },
    String {
        msg: String,
        source: anyhow::Error,
    },
}

impl fmt::Display for GdbmError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.to_string())
    }
}

fn get_error() -> (String, i32) {
    unsafe {
        let error_ptr = gdbm_errno_location();
        let raw_value = std::ptr::read(error_ptr);
        let error_ptr = gdbm_strerror(*error_ptr);
        let err_string = CStr::from_ptr(error_ptr);
        return (err_string.to_string_lossy().into_owned(), raw_value);
    }
}

fn datum(what: &str, data: impl AsRef<[u8]>) -> Result<datum, GdbmError> {
    let data = data.as_ref();
    if data.len() > i32::MAX as usize {
        return Err(GdbmError::new(format!("{} too large", what)));
    }
    // Note that we cast data.as_ptr(), which is a *const u8, to
    // a *mut i8. This is an artefact of the gdbm C interface where
    // 'dptr' is not 'const'. However gdbm does treat it as
    // const/immutable, so the cast is safe.
    Ok(datum {
        dptr: data.as_ptr() as *mut i8,
        dsize: data.len() as i32,
    })
}

bitflags! {
    pub struct Open:  c_uint{
        /// Read only database access
        const READER  = 0;
        /// Read and Write access to the database
        const WRITER  = 1;
        /// Read, write and create the database if it does not already exist
        const WRCREAT = 2;
        /// Create a new database regardless of whether one exised.  Gives read and write access
        const NEWDB   = 3;
        const FAST = 16;
        /// Sync all operations to disk
        const SYNC = 32;
        /// Prevents the library from locking the database file
        const NOLOCK = 64;
    }
}

bitflags! {
    pub struct Store:  c_uint{
        const INSERT  = 0;
        const REPLACE  = 1;
        const CACHESIZE  = 1;
        const FASTMODE  = 2;
        const SYNCMODE  = 3;
        const CENTFREE  = 4;
        const COALESCEBLKS  = 5;
    }
}

#[derive(Debug)]
pub struct Gdbm {
    db_handle: GDBM_FILE, /* int gdbm_export (GDBM_FILE, const char *, int, int);
                           * int gdbm_export_to_file (GDBM_FILE dbf, FILE *fp);
                           * int gdbm_import (GDBM_FILE, const char *, int);
                           * int gdbm_import_from_file (GDBM_FILE dbf, FILE *fp, int flag);
                           * int gdbm_count (GDBM_FILE dbf, gdbm_count_t *pcount);
                           * int gdbm_version_cmp (int const a[], int const b[]);
                           * */
}

// Safety: Gdbm does have thread-local data, but it's only used to set
// the gdbm_errno. We access that internally directly after calling
// into the gdbm library, it's not used to keep state besides that.
unsafe impl Send for Gdbm {}

impl Drop for Gdbm {
    fn drop(&mut self) {
        if self.db_handle.is_null() {
            // No cleanup needed
            return;
        }
        unsafe {
            gdbm_close(self.db_handle);
        }
    }
}

/// With locking disabled (if gdbm_open was called with ‘GDBM_NOLOCK’), the user may want
/// to perform their own file locking on the database file in order to prevent multiple
/// writers operating on the same file simultaneously.
impl AsRawFd for Gdbm {
    fn as_raw_fd(&self) -> RawFd {
        unsafe { gdbm_fdesc(self.db_handle) as RawFd }
    }
}

impl Gdbm {
    /// Open a DBM with location.
    /// mode (see http://www.manpagez.com/man/2/chmod,
    /// and http://www.manpagez.com/man/2/open), which is used if the file is created).
    pub fn new(path: &Path, block_size: u32, flags: Open, mode: i32) -> Result<Gdbm, GdbmError> {
        let path = CString::new(path.as_os_str().as_bytes())?;
        unsafe {
            let db_ptr = gdbm_open(
                path.as_ptr() as *mut i8,
                block_size as i32,
                flags.bits as i32,
                mode,
                None,
            );
            if db_ptr.is_null() {
                let raw_error = gdbm_errno_location();
                let raw_value = std::ptr::read(raw_error);
                return Err(GdbmError::String {
                    msg: "gdbm_open failed".to_string(),
                    source: anyhow!("gdbm_open failed").context(raw_value),
                });
            }
            Ok(Gdbm { db_handle: db_ptr })
        }
    }

    /// Store a record in the database.
    ///
    /// If `replace` is `false`, and the key already exists in the
    /// database, the record is not stored and `false` is returned.
    /// Otherwise `true` is returned.
    pub fn store(&self, key: &str, content: &String, replace: bool) -> Result<bool, GdbmError> {
        let key_datum = datum("key", key)?;
        let content_datum = datum("content", content)?;
        let flag = if replace {
            Store::REPLACE
        } else {
            Store::INSERT
        };
        let result =
            unsafe { gdbm_store(self.db_handle, key_datum, content_datum, flag.bits as i32) };
        if result < 0 {
            return Err(GdbmError::new(get_error()));
        }
        Ok(result == 0)
    }

    /// Retrieve a key from the database
    pub fn fetch(&self, key: &str) -> Result<String, GdbmError> {
        // datum gdbm_fetch(dbf, key);
        let key_datum = datum("key", key)?;
        unsafe {
            let content = gdbm_fetch(self.db_handle, key_datum);
            if content.dptr.is_null() {
                let (error_str, raw_value) = get_error();
                return Err(GdbmError::String {
                    msg: error_str,
                    source: anyhow!("gdbm_fetch failed").context(raw_value),
                });
            } else {
                // handle the data as an utf8 encoded string slice
                // that may or may not be terminated by a \0 byte.
                let ptr = content.dptr as *const u8;
                let len = content.dsize as usize;
                let mut slice = std::slice::from_raw_parts(ptr, len);
                if len > 0 && slice[len - 1] == 0 {
                    slice = &slice[0..len - 1];
                }
                let res = match std::str::from_utf8(slice) {
                    Ok(s) => Ok(s.to_string()),
                    Err(e) => Err(e.into()),
                };

                // Free the malloc'd content that the library gave us
                // Rust will manage this memory
                free(content.dptr as *mut c_void);

                return res;
            }
        }
    }

    /// Delete a key and value from the database
    pub fn delete(&self, key: &str) -> bool {
        let key_datum = match datum("key", key) {
            Ok(d) => d,
            Err(_) => return false,
        };
        unsafe {
            let result = gdbm_delete(self.db_handle, key_datum);
            if result == -1 {
                false
            } else {
                true
            }
        }
    }
    // TODO: Make an iterator out of this to hide the datum handling
    // pub fn firstkey(&self, key: &str) -> Result<String, GdbmError> {
    // unsafe {
    // let content = gdbm_firstkey(self.db_handle);
    // if content.dptr.is_null() {
    // return Err(GdbmError::new(get_error()));
    // } else {
    // let c_string = CStr::from_ptr(content.dptr);
    // let data = c_string.to_str()?.to_string();
    // Free the malloc'd content that the library gave us
    // Rust will manage this memory
    // free(content.dptr as *mut c_void);
    //
    // return Ok(data);
    // }
    // }
    // }
    // pub fn nextkey(&self, key: &str) -> Result<String, GdbmError> {
    // unsafe {
    // datum gdbm_nextkey(dbf, key);
    //
    // }
    // }
    //
    // int gdbm_reorganize(dbf);
    pub fn sync(&self) {
        unsafe {
            gdbm_sync(self.db_handle);
        }
    }

    /// Check to see if a key exists in the database
    pub fn exists(&self, key: &str) -> Result<bool, GdbmError> {
        let key_datum = datum("key", key)?;
        unsafe {
            let result = gdbm_exists(self.db_handle, key_datum);
            if result == 0 {
                Ok(true)
            } else {
                if gdbm_errno_location().is_null() {
                    return Ok(true);
                } else {
                    let (error_str, raw_value) = get_error();
                    return Err(GdbmError::String {
                        msg: error_str,
                        source: anyhow!("gdbm_exists failed").context(raw_value),
                    });
                }
            }
        }
    }
    // pub fn setopt(&self, key: &str) -> Result<(), GdbmError> {
    // unsafe {
    // int gdbm_setopt(dbf, option, value, size);
    //
    // }
    // }
    //
}

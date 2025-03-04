//! Safe interface to GNU dbm.
//!
//! The main documentation can be found at [`Gdbm`].
#[macro_use]
extern crate bitflags;
extern crate gdbm_sys;
extern crate libc;

use std::error::Error as StdError;
use std::io::Error;
use std::fmt;
use std::ffi::{CStr, CString, IntoStringError, NulError};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::Path;
use std::str::Utf8Error;
use std::string::FromUtf8Error;

use libc::{c_uint, c_void, free};

use gdbm_sys::*;

/// Custom error handling for the library
#[derive(Debug)]
pub enum GdbmError {
    FromUtf8Error(FromUtf8Error),
    Utf8Error(Utf8Error),
    NulError(NulError),
    Error(String),
    IoError(Error),
    IntoStringError(IntoStringError),
}

impl fmt::Display for GdbmError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl StdError for GdbmError {
    fn description(&self) -> &str {
        match *self {
            GdbmError::FromUtf8Error(ref _e) => "invalid utf-8 sequence",
            GdbmError::Utf8Error(ref _e) => "invalid utf-8 sequence",
            GdbmError::NulError(ref _e) => "nul byte found in provided data",
            GdbmError::Error(ref _e) => "gdbm error",
            GdbmError::IoError(ref _e) => "I/O error",
            GdbmError::IntoStringError(ref _e) => "error",
        }
    }
    fn cause(&self) -> Option<&dyn StdError> {
        match *self {
            GdbmError::FromUtf8Error(ref e) => e.source(),
            GdbmError::Utf8Error(ref e) => e.source(),
            GdbmError::NulError(ref e) => e.source(),
            GdbmError::Error(_) => None,
            GdbmError::IoError(ref e) => e.source(),
            GdbmError::IntoStringError(ref e) => e.source(),
        }
    }
}
impl GdbmError {
    /// Create a new GdbmError with a String message
    fn new(err: impl Into<String>) -> GdbmError {
        GdbmError::Error(err.into())
    }

    /// Convert a GdbmError into a String representation.
    pub fn to_string(&self) -> String {
        match *self {
            GdbmError::FromUtf8Error(ref err) => err.utf8_error().to_string(),
            GdbmError::Utf8Error(ref err) => err.to_string(),
            GdbmError::NulError(ref err) => err.to_string(),
            GdbmError::Error(ref err) => err.to_string(),
            GdbmError::IoError(ref err) => err.to_string(),
            GdbmError::IntoStringError(ref err) => err.to_string(),
        }
    }
}

impl From<NulError> for GdbmError {
    fn from(err: NulError) -> GdbmError {
        GdbmError::NulError(err)
    }
}
impl From<FromUtf8Error> for GdbmError {
    fn from(err: FromUtf8Error) -> GdbmError {
        GdbmError::FromUtf8Error(err)
    }
}
impl From<::std::str::Utf8Error> for GdbmError {
    fn from(err: ::std::str::Utf8Error) -> GdbmError {
        GdbmError::Utf8Error(err)
    }
}
impl From<IntoStringError> for GdbmError {
    fn from(err: IntoStringError) -> GdbmError {
        GdbmError::IntoStringError(err)
    }
}
impl From<Error> for GdbmError {
    fn from(err: Error) -> GdbmError {
        GdbmError::IoError(err)
    }
}


fn get_error() -> String {
    unsafe {
        let error_ptr = gdbm_strerror(*gdbm_errno_location());
        let err_string = CStr::from_ptr(error_ptr);
        return err_string.to_string_lossy().into_owned();
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
    /// Flags for [`Gdbm::new`]
    pub struct Open: c_uint {
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
    struct Store: c_uint {
        const INSERT  = 0;
        const REPLACE  = 1;
    }
}

/// An open `gdbm` database.
///
/// Note that a lot of the methods take arguments of type `impl AsRef<[u8]>`.
/// This means you can pass types `String`, `&str`, `&String`, `&[u8]` directly.
///
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
        unsafe {
            gdbm_fdesc(self.db_handle) as RawFd
        }
    }
}

impl Gdbm {
    /// Open a DBM with location.
    ///
    /// `mode` is the unix mode used when a database is created. See
    /// [chmod](http://www.manpagez.com/man/2/chmod) and
    /// [open](http://www.manpagez.com/man/2/open).
    pub fn new(
        path: impl AsRef<Path>,
        block_size: u32,
        flags: Open,
        mode: u32
    ) -> Result<Gdbm, GdbmError> {
        if block_size > i32::MAX as u32 {
            return Err(GdbmError::new("block_size too large"));
        }
        if mode > i32::MAX as u32 {
            return Err(GdbmError::new("invalid mode"));
        }
        let path = CString::new(path.as_ref().as_os_str().as_bytes())?;
        unsafe {
            let db_ptr = gdbm_open(path.as_ptr() as *mut i8,
                                   block_size as i32,
                                   flags.bits as i32,
                                   mode as i32,
                                   None);
            if db_ptr.is_null() {
                return Err(GdbmError::new("gdbm_open failed".to_string()));
            }
            Ok(Gdbm { db_handle: db_ptr })
        }
    }

    /// Store a record in the database.
    ///
    /// If `replace` is `false`, and the key already exists in the
    /// database, the record is not stored and `false` is returned.
    /// Otherwise `true` is returned.
    pub fn store(
        &self,
        key: impl AsRef<[u8]>,
        content: impl AsRef<[u8]>,
        replace: bool,
    ) -> Result<bool, GdbmError> {
        let key_datum = datum("key", key)?;
        let content_datum = datum("content", content)?;
        let flag = if replace { Store::REPLACE } else { Store::INSERT };
        let result = unsafe {
            gdbm_store(self.db_handle, key_datum, content_datum, flag.bits as i32)
        };
        if result < 0 {
            return Err(GdbmError::new(get_error()));
        }
        Ok(result == 0)
    }

    /// Retrieve a record from the database.
    pub fn fetch_data(&self, key: impl AsRef<[u8]>) -> Result<Vec<u8>, GdbmError> {
        // datum gdbm_fetch(dbf, key);
        let key_datum = datum("key", key)?;
        unsafe {
            let content = gdbm_fetch(self.db_handle, key_datum);
            if content.dptr.is_null() {
                return Err(GdbmError::new(get_error()));
            } else if content.dsize < 0 {
                return Err(GdbmError::new("content has negative size"));
            } else {
                let ptr = content.dptr as *const u8;
                let len = content.dsize as usize;
                let slice = std::slice::from_raw_parts(ptr, len);
                let vec = slice.to_vec();

                // Free the malloc'd content that the library gave us
                // Rust will manage this memory
                free(content.dptr as *mut c_void);

                return Ok(vec);
            }
        }
    }

    /// Retrieve a string record from the database.
    ///
    /// If it exists, but the content is not a valid UTF-8 string,
    /// `FromUtf8Error` will be returned.
    pub fn fetch_string(&self, key: impl AsRef<[u8]>) -> Result<String, GdbmError> {
        let vec = self.fetch_data(key)?;
        let s = String::from_utf8(vec)?;
        Ok(s)
    }

    /// Retrieve a string record from the database and strip trailing '\0'.
    ///
    /// C libraries might store strings with their trailing '\0' byte.
    /// This method is like `fetch_string` but it strips that byte.
    pub fn fetch_cstring(&self, key: impl AsRef<[u8]>) -> Result<String, GdbmError> {
        let vec = self.fetch_data(key)?;
        let mut s = String::from_utf8(vec)?;
        if s.ends_with("\0") {
            s.pop();
        }
        Ok(s)
    }

    /// Delete a record from the database.
    ///
    /// Returns `false` if the key was not present, `true` if it
    /// was present and the record was deleted.
    pub fn delete(&self, key: impl AsRef<[u8]>) -> Result<bool, GdbmError> {
        let key_datum = datum("key", key)?;
        let result = unsafe {
            gdbm_delete(self.db_handle, key_datum)
        };
        if result < 0 {
            if unsafe { *gdbm_errno_location() } == 0 {
                return Ok(false);
            }
            return Err(GdbmError::new(get_error()));
        }
        Ok(true)
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

    /// Check to see if a record with this key exists in the database
    pub fn exists(&self, key: impl AsRef<[u8]>) -> Result<bool, GdbmError> {
        let key_datum = datum("key", key)?;
        unsafe {
            let result = gdbm_exists(self.db_handle, key_datum);
            if result == 0 {
                Ok(true)
            } else {
                if *gdbm_errno_location() == 0 {
                    return Ok(true);
                } else {
                    return Err(GdbmError::new(get_error()));
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

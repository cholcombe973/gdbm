#[macro_use]
extern crate bitflags;
extern crate gdbm_sys;
extern crate libc;

use std::error::Error as err;
use std::io::Error;
use std::fmt;
use std::ffi::{CStr, CString, IntoStringError, NulError};
use std::os::unix::ffi::OsStrExt;
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
        f.write_str(self.description())
    }
}

impl err for GdbmError {
    fn description(&self) -> &str {
        match *self {
            GdbmError::FromUtf8Error(ref e) => e.description(),
            GdbmError::Utf8Error(ref e) => e.description(),
            GdbmError::NulError(ref e) => e.description(),
            GdbmError::Error(ref e) => &e,
            GdbmError::IoError(ref e) => e.description(),
            GdbmError::IntoStringError(ref e) => e.description(),
        }
    }
    fn cause(&self) -> Option<&err> {
        match *self {
            GdbmError::FromUtf8Error(ref e) => e.cause(),
            GdbmError::Utf8Error(ref e) => e.cause(),
            GdbmError::NulError(ref e) => e.cause(),
            GdbmError::Error(_) => None,
            GdbmError::IoError(ref e) => e.cause(),
            GdbmError::IntoStringError(ref e) => e.cause(),
        }
    }
}
impl GdbmError {
    /// Create a new GdbmError with a String message
    fn new(err: String) -> GdbmError {
        GdbmError::Error(err)
    }

    /// Convert a GdbmError into a String representation.
    pub fn to_string(&self) -> String {
        match *self {
            GdbmError::FromUtf8Error(ref err) => err.utf8_error().to_string(),
            GdbmError::Utf8Error(ref err) => err.description().to_string(),
            GdbmError::NulError(ref err) => err.description().to_string(),
            GdbmError::Error(ref err) => err.to_string(),
            GdbmError::IoError(ref err) => err.description().to_string(),
            GdbmError::IntoStringError(ref err) => err.description().to_string(),
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
        let error_ptr = gdbm_strerror(gdbm_errno);
        let err_string = CStr::from_ptr(error_ptr);
        return err_string.to_string_lossy().into_owned();
    }
}

bitflags! {
    pub flags Flags:  c_uint{
        /// Read only database access
        const READER  = 0,
        /// Read and Write access to the database
        const WRITER  = 1,
        /// Read, write and create the database if it does not already exist
        const WRCREAT = 2,
        /// Create a new database regardless of whether one exised.  Gives read and write access
        const NEWDB   = 3,
        const FAST = 16,
        /// Sync all operations to disk
        const SYNC = 32,
        /// Prevents the library from locking the database file
        const NOLOCK = 64,
    }
}

bitflags! {
    pub flags StoreFlags:  c_uint{
        const INSERT  = 0,
        const REPLACE  = 1,
        const CACHESIZE  = 1,
        const FASTMODE  = 2,
        const SYNCMODE  = 3,
        const CENTFREE  = 4,
        const COALESCEBLKS  = 5,
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

impl Gdbm {
    /// Open a DBM with location.
    /// mode (see http://www.manpagez.com/man/2/chmod,
    /// and http://www.manpagez.com/man/2/open), which is used if the file is created).
    pub fn new(path: &Path, block_size: u32, flags: Flags, mode: i32) -> Result<Gdbm, GdbmError> {
        let path = CString::new(path.as_os_str().as_bytes())?;
        unsafe {
            let db_ptr = gdbm_open(path.as_ptr() as *mut i8,
                                   block_size as i32,
                                   flags.bits as i32,
                                   mode,
                                   None);
            if db_ptr.is_null() {
                return Err(GdbmError::new("gdbm_open failed".to_string()));
            }
            Ok(Gdbm { db_handle: db_ptr })
        }
    }
    /// This function returns either -1, 0 or +1.
    /// -1 means the item was not stored.  0 means it was stored and +1 means it was
    /// not stored because the key already existed.  See the link below for more details.
    /// http://www.gnu.org.ua/software/gdbm/manual/gdbm.html#Store
    pub fn store(&self, key: &str, content: &mut String, flag: StoreFlags) -> i32 {
        let key_datum = datum {
            dptr: key.as_ptr() as *mut i8,
            dsize: key.len() as i32,
        };
        let content_datum = datum {
            dptr: content.as_ptr() as *mut i8,
            dsize: content.len() as i32,
        };
        unsafe {
            let result = gdbm_store(self.db_handle, key_datum, content_datum, flag.bits as i32);
            result
        }
    }

    /// Retrieve a key from the database
    pub fn fetch(&self, key: &str) -> Result<String, GdbmError> {
        // datum gdbm_fetch(dbf, key);
        let key_datum = datum {
            dptr: key.as_ptr() as *mut i8,
            dsize: key.len() as i32,
        };
        unsafe {
            let content = gdbm_fetch(self.db_handle, key_datum);
            if content.dptr.is_null() {
                return Err(GdbmError::new(get_error()));
            } else {
                let c_string = CStr::from_ptr(content.dptr);
                let data = c_string.to_str()?.to_string();
                // Free the malloc'd content that the library gave us
                // Rust will manage this memory
                free(content.dptr as *mut c_void);

                return Ok(data);
            }
        }
    }

    /// Delete a key and value from the database
    pub fn delete(&self, key: &str) -> bool {
        let key_datum = datum {
            dptr: key.as_ptr() as *mut i8,
            dsize: key.len() as i32,
        };
        unsafe {
            let result = gdbm_delete(self.db_handle, key_datum);
            if result == -1 { false } else { true }
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
        let key_datum = datum {
            dptr: key.as_ptr() as *mut i8,
            dsize: key.len() as i32,
        };
        unsafe {
            let result = gdbm_exists(self.db_handle, key_datum);
            if result == 0 {
                Ok(true)
            } else {
                if gdbm_errno == 0 {
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

    /// With locking disabled (if gdbm_open was called with ‘GDBM_NOLOCK’), the user may want
    /// to perform their own file locking on the database file in order to prevent multiple
    /// writers operating on the same file simultaneously.
    pub fn fdesc(&self) -> ::std::os::raw::c_int {
        unsafe {
            let file_id = gdbm_fdesc(self.db_handle);
            file_id
        }
    }
}

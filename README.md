# gdbm
Rust gdbm safe interface

## Compiling
This library requires at least gdbm 1.14.

If you are running an executable that was compiled with this crate as a
dependency, only the shared library needs to be available at runtime.
For Debian and derivatives, that means the libgdbm6 package. If you're
compiling you'll also need the libgdbm-dev package.

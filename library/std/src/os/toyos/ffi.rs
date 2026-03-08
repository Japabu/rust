//! ToyOS-specific extension to the primitives in the `std::ffi` module

use crate::ffi::{OsStr, OsString};
use crate::mem;
use crate::sealed::Sealed;
use crate::sys::os_str::Buf;
use crate::sys::{AsInner, FromInner, IntoInner};

/// Platform-specific extensions to [`OsString`].
///
/// This trait is sealed: it cannot be implemented outside the standard library.
/// This is so that future additional methods are not breaking changes.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub trait OsStringExt: Sealed {
    /// Creates an [`OsString`] from a byte vector.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    fn from_vec(vec: Vec<u8>) -> Self;

    /// Yields the underlying byte vector of this [`OsString`].
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    fn into_vec(self) -> Vec<u8>;
}

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl OsStringExt for OsString {
    #[inline]
    fn from_vec(vec: Vec<u8>) -> OsString {
        FromInner::from_inner(Buf { inner: vec })
    }
    #[inline]
    fn into_vec(self) -> Vec<u8> {
        self.into_inner().inner
    }
}

/// Platform-specific extensions to [`OsStr`].
///
/// This trait is sealed: it cannot be implemented outside the standard library.
/// This is so that future additional methods are not breaking changes.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub trait OsStrExt: Sealed {
    /// Creates an [`OsStr`] from a byte slice.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    fn from_bytes(slice: &[u8]) -> &Self;

    /// Gets the underlying byte view of the [`OsStr`] slice.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    fn as_bytes(&self) -> &[u8];
}

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl OsStrExt for OsStr {
    #[inline]
    fn from_bytes(slice: &[u8]) -> &OsStr {
        unsafe { mem::transmute(slice) }
    }
    #[inline]
    fn as_bytes(&self) -> &[u8] {
        &self.as_inner().inner
    }
}

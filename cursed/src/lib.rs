use std::{
    error::Error,
    marker::PhantomData,
    sync::Arc,
    borrow::Cow,
    ffi::{CStr, CString},
    convert::Infallible,
    io
};

#[macro_export]
macro_rules! throw {
    ($error:path, $ex:ident, $fallback:expr) => {{
        use std::default::Default;

        if let Some(callback) = $ex {
            let err = format!("{:?}", $error);
            let s = std::ffi::CString::new(err)
                .unwrap_or_else(|_| std::ffi::CString::new("<unknown>".to_string()).unwrap());
            callback(s.as_ptr());
        }

        $fallback
    }};

    ($error:path, $ex:ident) => {
        $crate::throw!($error, $ex, ())
    };
}

#[macro_export]
macro_rules! try_not_null {
    ($path:expr, $ex:ident, $fallback:expr) => {{
        match $path {
            Ok(v) => v,
            Err(e) => {
                return $crate::throw!(e, $ex, $fallback);
            }
        }
    }};

    ($path:path, $ex:ident) => {
        try_not_null!($path, $ex, ())
    };
}

pub type ErrCallback = Option<extern "C" fn(*const libc::c_char)>;

pub trait ToForeign<Local, Foreign>: Sized {
    type Error;
    fn to_foreign(_: Local) -> Result<Foreign, Self::Error>;
    fn drop_foreign(_: Foreign) {}
}

pub trait FromForeign<Foreign, Local>: Sized {
    type Error;
    fn from_foreign(_: Foreign) -> Result<Local, Self::Error>;
    fn drop_local(_: Local) {}
}

#[inline(always)]
fn null_ptr_error() -> Box<io::Error> {
    Box::new(io::Error::new(io::ErrorKind::InvalidData, "null pointer"))
}

/// Magical catch-all implementation for `Result<Local, Error>`.
impl<T, Foreign, Local, Error> ToForeign<Result<Local, Error>, Foreign> for T
where
    T: ToForeign<Local, Foreign, Error = Error>
{
    type Error = Error;

    fn to_foreign(result: Result<Local, Error>) -> Result<Foreign, Self::Error> {
        match result {
            Ok(v) => <Self as ToForeign<Local, Foreign>>::to_foreign(v),
            Err(e) => Err(e)
        }
    }
}

pub struct BoxMarshaler<T: ?Sized>(PhantomData<T>);

impl<T: ?Sized> FromForeign<*mut T, Box<T>> for BoxMarshaler<T> {
    type Error = Box<dyn Error>;

    #[inline(always)]
    fn from_foreign(box_ptr: *mut T) -> Result<Box<T>, Self::Error> {
        if box_ptr.is_null() {
            return Err(null_ptr_error());
        }

        Ok(unsafe { Box::from_raw(box_ptr) })
    }
}

impl<T: ?Sized> ToForeign<Box<T>, *mut T> for BoxMarshaler<T> {
    type Error = Box<dyn Error>;

    #[inline(always)]
    fn to_foreign(boxed: Box<T>) -> Result<*mut T, Self::Error> {
        Ok(Box::into_raw(boxed))
    }
}

pub struct ArcMarshaler<T: ?Sized>(PhantomData<T>);

impl<T: ?Sized> FromForeign<*const T, Arc<T>> for ArcMarshaler<T> {
    type Error = Box<dyn Error>;

    #[inline(always)]
    fn from_foreign(arc_ptr: *const T) -> Result<Arc<T>, Self::Error> {
        if arc_ptr.is_null() {
            return Err(null_ptr_error());
        }

        Ok(unsafe { Arc::from_raw(arc_ptr) })
    }
}

impl<T: ?Sized> ToForeign<Arc<T>, *const T> for ArcMarshaler<T> {
    type Error = Arc<dyn Error>;

    #[inline(always)]
    fn to_foreign(arced: Arc<T>) -> Result<*const T, Self::Error> {
        Ok(Arc::into_raw(arced))
    }
}

pub struct BoolMarshaler;

impl FromForeign<u8, bool> for BoolMarshaler {
    type Error = Infallible;

    #[inline(always)]
    fn from_foreign(i: u8) -> Result<bool, Self::Error> {
        Ok(i != 0)
    }
}

impl ToForeign<bool, u8> for BoolMarshaler {
    type Error = std::convert::Infallible;

    #[inline(always)]
    fn to_foreign(b: bool) -> Result<u8, Self::Error> {
        Ok(if b { 1 } else { 0 })
    }
}

pub struct StrMarshaler<'a>(&'a PhantomData<()>);

impl<'a> FromForeign<*const libc::c_char, Cow<'a, str>> for StrMarshaler<'a> {
    type Error = Box<dyn Error>;

    fn from_foreign(key: *const libc::c_char) -> Result<Cow<'a, str>, Self::Error> {
        if key.is_null() {
            return Err(null_ptr_error());
        }
        Ok(unsafe { CStr::from_ptr(key) }.to_string_lossy())
    }
}

impl<'a> ToForeign<&'a str, *const libc::c_char> for StrMarshaler<'a> {
    type Error = Box<dyn Error>;

    fn to_foreign(input: &'a str) -> Result<*const libc::c_char, Self::Error> {
        let c_str = CString::new(input)?;
        Ok(c_str.into_raw())
    }

    fn drop_foreign(ptr: *const libc::c_char) {
        if !ptr.is_null() {
            unsafe { CString::from_raw(ptr as *mut _) };
        }
    }
}
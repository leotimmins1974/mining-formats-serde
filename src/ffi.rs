//! Stable C ABI used by the managed .NET wrapper.

use std::{
    cell::RefCell,
    ffi::{CString, c_char},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use crate::{
    Document,
    format::{self, ByteStreams},
};

const STATUS_SUCCESS: i32 = 0;
const STATUS_INVALID_ARGUMENT: i32 = 1;
const STATUS_CONVERSION_ERROR: i32 = 2;
const STATUS_PANIC: i32 = 3;

thread_local! {
    static LAST_ERROR: RefCell<CString> = RefCell::new(CString::default());
}

#[repr(C)]
pub struct Version {
    major: u32,
    minor: u32,
    patch: u32,
}

pub struct NativeDocument {
    document: Document,
}

pub struct NativeBuffers {
    streams: ByteStreams,
}

#[derive(Clone, Copy)]
enum Format {
    Omf,
    Obj,
    Las,
    Laz,
    Tiff,
}

impl TryFrom<u32> for Format {
    type Error = Failure;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Omf),
            1 => Ok(Self::Obj),
            2 => Ok(Self::Las),
            3 => Ok(Self::Laz),
            4 => Ok(Self::Tiff),
            _ => Err(Failure::invalid(format!("unknown format value {value}"))),
        }
    }
}

struct Failure {
    status: i32,
    message: String,
}

impl Failure {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            status: STATUS_INVALID_ARGUMENT,
            message: message.into(),
        }
    }

    fn conversion(error: impl std::fmt::Display) -> Self {
        Self {
            status: STATUS_CONVERSION_ERROR,
            message: error.to_string(),
        }
    }
}

fn set_last_error(message: &str) {
    let message = CString::new(message.replace('\0', "\\0")).unwrap_or_default();
    LAST_ERROR.with(|slot| *slot.borrow_mut() = message);
}

fn clear_last_error() {
    LAST_ERROR.with(|slot| *slot.borrow_mut() = CString::default());
}

fn invoke(operation: impl FnOnce() -> Result<(), Failure>) -> i32 {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(())) => {
            clear_last_error();
            STATUS_SUCCESS
        }
        Ok(Err(failure)) => {
            set_last_error(&failure.message);
            failure.status
        }
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("unknown Rust panic");
            set_last_error(&format!("internal MFSD panic: {message}"));
            STATUS_PANIC
        }
    }
}

fn deserialize(format: Format, bytes: &[u8]) -> Result<Document, Failure> {
    match format {
        Format::Omf => format::omf::deserialize(bytes).map_err(Failure::conversion),
        Format::Obj => format::obj::deserialize(bytes).map_err(Failure::conversion),
        Format::Las => format::las::deserialize(bytes).map_err(Failure::conversion),
        Format::Laz => format::laz::deserialize(bytes).map_err(Failure::conversion),
        Format::Tiff => format::tiff::deserialize(bytes).map_err(Failure::conversion),
    }
}

fn serialize(format: Format, document: &Document) -> Result<ByteStreams, Failure> {
    match format {
        Format::Omf => Ok(format::omf::serialize(document)),
        Format::Obj => format::obj::serialize(document).map_err(Failure::conversion),
        Format::Las => format::las::serialize(document).map_err(Failure::conversion),
        Format::Laz => format::laz::serialize(document).map_err(Failure::conversion),
        Format::Tiff => format::tiff::serialize(document).map_err(Failure::conversion),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn version() -> Version {
    Version {
        major: env!("CARGO_PKG_VERSION_MAJOR").parse().unwrap_or_default(),
        minor: env!("CARGO_PKG_VERSION_MINOR").parse().unwrap_or_default(),
        patch: env!("CARGO_PKG_VERSION_PATCH").parse().unwrap_or_default(),
    }
}

/// Returns a borrowed UTF-8 error message for the most recent failed call on
/// the current thread. The pointer remains valid until another ABI call on the
/// same thread changes the error.
#[unsafe(no_mangle)]
pub extern "C" fn mfsd_last_error_message() -> *const c_char {
    LAST_ERROR.with(|slot| slot.borrow().as_ptr())
}

/// Creates an opaque document handle. `data` is borrowed only for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfsd_document_deserialize(format: u32, data: *const u8, length: usize, out_document: *mut *mut NativeDocument) -> i32 {
    invoke(|| {
        if out_document.is_null() {
            return Err(Failure::invalid("out_document must not be null"));
        }
        // SAFETY: The caller supplied a writable output pointer as required by
        // the ABI contract. It is initialized before any fallible operation.
        unsafe { out_document.write(ptr::null_mut()) };

        let bytes = if length == 0 {
            &[]
        } else {
            if data.is_null() {
                return Err(Failure::invalid("data must not be null when length is non-zero"));
            }
            // SAFETY: The caller guarantees that `data` points to `length`
            // readable bytes for the duration of this call.
            unsafe { std::slice::from_raw_parts(data, length) }
        };
        let format = Format::try_from(format)?;
        let document = Box::new(NativeDocument {
            document: deserialize(format, bytes)?,
        });
        // SAFETY: `out_document` was validated above. Ownership of this box is
        // transferred to the caller and reclaimed by `mfsd_document_free`.
        unsafe { out_document.write(Box::into_raw(document)) };
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfsd_document_element_count(document: *const NativeDocument, out_count: *mut usize) -> i32 {
    invoke(|| {
        if out_count.is_null() {
            return Err(Failure::invalid("out_count must not be null"));
        }
        // SAFETY: The caller supplied a writable output pointer.
        unsafe { out_count.write(0) };
        // SAFETY: Handles returned by this library remain valid until freed.
        let document = unsafe { document.as_ref() }.ok_or_else(|| Failure::invalid("document must not be null"))?;
        // SAFETY: `out_count` was validated above.
        unsafe { out_count.write(document.document.project().elements.len()) };
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfsd_document_serialize(document: *const NativeDocument, format: u32, out_buffers: *mut *mut NativeBuffers) -> i32 {
    invoke(|| {
        if out_buffers.is_null() {
            return Err(Failure::invalid("out_buffers must not be null"));
        }
        // SAFETY: The caller supplied a writable output pointer as required by
        // the ABI contract. It is initialized before any fallible operation.
        unsafe { out_buffers.write(ptr::null_mut()) };
        // SAFETY: Handles returned by this library remain valid until freed.
        let document = unsafe { document.as_ref() }.ok_or_else(|| Failure::invalid("document must not be null"))?;
        let format = Format::try_from(format)?;
        let buffers = Box::new(NativeBuffers {
            streams: serialize(format, &document.document)?,
        });
        // SAFETY: `out_buffers` was validated above. Ownership is transferred
        // to the caller and reclaimed by `mfsd_buffers_free`.
        unsafe { out_buffers.write(Box::into_raw(buffers)) };
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfsd_buffers_count(buffers: *const NativeBuffers, out_count: *mut usize) -> i32 {
    invoke(|| {
        if out_count.is_null() {
            return Err(Failure::invalid("out_count must not be null"));
        }
        // SAFETY: The caller supplied a writable output pointer.
        unsafe { out_count.write(0) };
        // SAFETY: Handles returned by this library remain valid until freed.
        let buffers = unsafe { buffers.as_ref() }.ok_or_else(|| Failure::invalid("buffers must not be null"))?;
        // SAFETY: `out_count` was validated above.
        unsafe { out_count.write(buffers.streams.len()) };
        Ok(())
    })
}

/// Borrows one serialized stream until `buffers` is freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfsd_buffers_get(buffers: *const NativeBuffers, index: usize, out_data: *mut *const u8, out_length: *mut usize) -> i32 {
    invoke(|| {
        if out_data.is_null() || out_length.is_null() {
            return Err(Failure::invalid("out_data and out_length must not be null"));
        }
        // SAFETY: The caller supplied writable output pointers.
        unsafe {
            out_data.write(ptr::null());
            out_length.write(0);
        }
        // SAFETY: Handles returned by this library remain valid until freed.
        let buffers = unsafe { buffers.as_ref() }.ok_or_else(|| Failure::invalid("buffers must not be null"))?;
        let stream = buffers
            .streams
            .get(index)
            .ok_or_else(|| Failure::invalid(format!("buffer index {index} is out of range")))?;
        // SAFETY: The outputs were validated above. The returned data remains
        // owned by `buffers` until `mfsd_buffers_free` is called.
        unsafe {
            out_data.write(stream.as_ptr());
            out_length.write(stream.len());
        }
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfsd_document_free(document: *mut NativeDocument) {
    if !document.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: The pointer came from `Box::into_raw` in this library and
            // the caller must release it exactly once.
            drop(unsafe { Box::from_raw(document) });
        }));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfsd_buffers_free(buffers: *mut NativeBuffers) {
    if !buffers.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: The pointer came from `Box::into_raw` in this library and
            // the caller must release it exactly once.
            drop(unsafe { Box::from_raw(buffers) });
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffi_obj_to_omf_round_trip() {
        let input = b"o triangle\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
        let mut document = ptr::null_mut();
        // SAFETY: Every pointer and length in this test refers to live local
        // storage, and every returned handle is released exactly once.
        unsafe {
            assert_eq!(mfsd_document_deserialize(Format::Obj as u32, input.as_ptr(), input.len(), &mut document), STATUS_SUCCESS);
            assert!(!document.is_null());

            let mut count = 0;
            assert_eq!(mfsd_document_element_count(document, &mut count), STATUS_SUCCESS);
            assert_eq!(count, 1);

            let mut buffers = ptr::null_mut();
            assert_eq!(mfsd_document_serialize(document, Format::Omf as u32, &mut buffers), STATUS_SUCCESS);
            assert!(!buffers.is_null());

            assert_eq!(mfsd_buffers_count(buffers, &mut count), STATUS_SUCCESS);
            assert_eq!(count, 1);

            let mut data = ptr::null();
            let mut length = 0;
            assert_eq!(mfsd_buffers_get(buffers, 0, &mut data, &mut length), STATUS_SUCCESS);
            assert!(!data.is_null());
            assert!(length > 0);

            mfsd_buffers_free(buffers);
            mfsd_document_free(document);
        }
    }

    #[test]
    fn ffi_reports_invalid_format() {
        let mut document = ptr::null_mut();
        // SAFETY: The output pointer refers to live local storage.
        let status = unsafe { mfsd_document_deserialize(u32::MAX, ptr::null(), 0, &mut document) };
        assert_eq!(status, STATUS_INVALID_ARGUMENT);
        assert!(document.is_null());
        // SAFETY: The error pointer is owned by the current thread and remains
        // valid until the next ABI call on this thread.
        let message = unsafe { std::ffi::CStr::from_ptr(mfsd_last_error_message()) };
        assert!(message.to_string_lossy().contains("unknown format"));
    }
}

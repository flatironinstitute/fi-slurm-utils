use std::{ffi::CString, os::raw::c_void};

use fi_slurm_sys::{
    list_itr_t, slurm_list_append, slurm_list_create, slurm_list_iterator_create,
    slurm_list_iterator_destroy, slurm_list_next, xlist,
};

/// A custom destructor function that can be passed to C
/// It takes a raw pointer to a CString and correctly frees it using Rust's allocator
#[unsafe(no_mangle)]
extern "C" fn free_rust_string(ptr: *mut c_void) {
    if !ptr.is_null() {
        unsafe {
            // Reconstruct the CString from the raw pointer and let it drop,
            // which correctly deallocates the memory
            let _ = CString::from_raw(ptr as *mut i8);
        }
    }
}

/// Converts an optional Vec<String> into a Slurm-compatible C list
/// If the input is None, returns a null pointer.
/// If the input is Some(vec), creates a Slurm list containing the strings
/// # Safety
/// This function is unsafe because it interacts with raw pointers and C memory management.
/// The caller must ensure that the returned pointer is eventually freed using Slurm's
/// `slurm_list_destroy` function to avoid memory leaks.
pub unsafe fn vec_to_slurm_list(data: Option<Vec<String>>) -> *mut xlist {
    // If the Option is None, we return a null pointer, which Slurm ignores
    let Some(vec) = data else {
        return std::ptr::null_mut();
    };

    // If the vector is not empty, create a Slurm list
    let slurm_list = unsafe { slurm_list_create(Some(free_rust_string)) };
    // If Slurm fails to allocate, return null for safety
    if slurm_list.is_null() {
        return std::ptr::null_mut(); // returning the null is fine in this case, it's part of the
        // expected API, the equivalent of an Option resolving to None
    }
    for item in vec {
        // sanitize interior NULs so CString::new never fails
        let safe = item.replace('\0', "");
        let c_string = CString::new(safe).unwrap();
        // Give ownership of the string memory to the C list
        // The list's destructor will free it
        unsafe { slurm_list_append(slurm_list, c_string.into_raw() as *mut c_void) };
    }
    slurm_list
}

/// Helper function for quickly converting between ints and bools
pub fn bool_to_int(b: bool) -> u16 {
    if b { 1 } else { 0 }
}

/// A container struct for a pointer to a C list iterator
pub struct SlurmIterator {
    pub ptr: *mut list_itr_t,
}

impl SlurmIterator {
    /// Create a new slurm list from a raw pointer
    /// # Safety
    /// This function is unsafe because it takes a raw pointer to a C list.
    /// The caller must ensure that the pointer is valid and points to a properly initialized
    /// Slurm list. If the pointer is null, the iterator will also be null.
    /// The caller is responsible for ensuring that the list outlives the iterator.
    /// The iterator must be dropped to free its resources.///
    pub unsafe fn new(list_ptr: *mut xlist) -> Self {
        if list_ptr.is_null() {
            return Self {
                ptr: std::ptr::null_mut(),
            };
        }
        let iter_ptr = unsafe { slurm_list_iterator_create(list_ptr) };

        Self { ptr: iter_ptr }
    }
}

impl Drop for SlurmIterator {
    /// Safely destroy the slurm list iterator by freeing it with the C destructor
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                slurm_list_iterator_destroy(self.ptr);
            }
            self.ptr = std::ptr::null_mut();
        }
    }
}

impl Iterator for SlurmIterator {
    type Item = *mut c_void;

    fn next(&mut self) -> Option<Self::Item> {
        // encapsulating an unsafe C-style loop
        if self.ptr.is_null() {
            return None;
        };
        unsafe {
            let node_ptr = slurm_list_next(self.ptr);

            // converting the outcome to an Option
            if node_ptr.is_null() {
                // C iterators end with a null, so we return None
                None
            } else {
                Some(node_ptr)
            }
        }
    }
}

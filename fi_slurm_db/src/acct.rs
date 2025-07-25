// prototyping the ability to grab database information

// example, replicating sacctmgr show qos cca format=MaxTRESPU%20,GrpTRES%20
//
// first, get user name, likely from OS env
// second, query for the user's associated accounts
// third, for each of those accounts, 

// API guidelines: for anything that would be passed from Rust to C, use Box to 
// create the pointers and pass control to C, rather than trying to create the 
// C structs locally and passing them in by value, since this creates
// use after frees in interaction with manual Drop implementations

// observe the AssocConfig struct and its implementations for a guide on how we want
// this API to work: an into_c_struct method should take fn(self) -> c_struct, initializing zeroed
// memory internally, populating it from the struct, and then allocating it onto the heap with
// Box::into_raw(Box::new(c_struct)). These Config types should not have manual Drop
// implementations: when we pass the memory to C, it becomes C's responsibility to free, don't 
// tempt a double-free by adding one

use std::{
    ffi::{CStr, CString}, 
    ops::{Deref, DerefMut}, 
    os::raw::c_void
};
use chrono::{DateTime, Utc, Duration};

use rust_bind::bindings::{list_itr_t, slurm_list_append, slurm_list_create, slurm_list_destroy, slurm_list_iterator_create, slurm_list_iterator_destroy, slurm_list_next, slurmdb_assoc_cond_t, slurmdb_assoc_rec_t, slurmdb_connection_close, slurmdb_connection_get, slurmdb_qos_cond_t, slurmdb_qos_get, slurmdb_qos_rec_t, slurmdb_user_cond_t, slurmdb_user_rec_t, slurmdb_users_get, xlist};

use thiserror::Error;

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

#[derive(Error, Debug)]
enum DbConnError {
    #[error("Could not establish connection to SlurmDB. Please ensure that SlurmDB is present and slurm_init has been run.")]
    DbConnectionError,
}

struct DbConn {
    ptr: *mut c_void,
}

impl DbConn {
    fn new(persist_flags: &mut u16) -> Result<Self, DbConnError> {
        unsafe {
            let ptr = slurmdb_connection_get(persist_flags);

            if !ptr.is_null() {
                Ok(Self {
                    ptr
                })
            } else {
                Err(DbConnError::DbConnectionError)
            }
        }
    }
    
    fn as_mut_ptr(&mut self) -> *mut c_void { self.ptr }
}

impl Drop for DbConn {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                slurmdb_connection_close(&mut self.ptr);
            }
            self.ptr = std::ptr::null_mut();
        }
    }
}

unsafe fn slurmdb_connect(persist_flags: &mut u16) -> Result<DbConn, DbConnError> {
    DbConn::new(persist_flags)
}

fn bool_to_int(b: bool) -> u16 {
    if b {
        1
    } else {
        0
    }
}

unsafe fn vec_to_slurm_list(data: Option<Vec<String>>) -> *mut xlist {
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

struct AssocConfig {
    acct_list: Option<Vec<String>>,
    cluster_list: Option<Vec<String>>,
    def_qos_id_list: Option<Vec<String>>,
    flags: u32, // bitflags
    format_list: Option<Vec<String>>,
    id_list: Option<Vec<String>>,
    parent_acct_list: Option<Vec<String>>,
    partition_list: Option<Vec<String>>,
    qos_list: Option<Vec<String>>,
    usage_end: DateTime<Utc>,
    usage_start: DateTime<Utc>,
    user_list: Option<Vec<String>>,
}

impl AssocConfig {
    fn into_c_struct(self) -> slurmdb_assoc_cond_t {
        unsafe {
            let mut c_struct: slurmdb_assoc_cond_t = std::mem::zeroed();

            c_struct.acct_list = vec_to_slurm_list(self.acct_list);
            c_struct.cluster_list = vec_to_slurm_list(self.cluster_list);
            c_struct.def_qos_id_list = vec_to_slurm_list(self.def_qos_id_list);
            c_struct.flags = self.flags;
            c_struct.format_list = vec_to_slurm_list(self.format_list);
            c_struct.id_list = vec_to_slurm_list(self.id_list);
            c_struct.parent_acct_list = vec_to_slurm_list(self.parent_acct_list);
            c_struct.partition_list = vec_to_slurm_list(self.partition_list);
            c_struct.qos_list = vec_to_slurm_list(self.qos_list);
            c_struct.usage_end = self.usage_end.timestamp();
            c_struct.usage_start = self.usage_start.timestamp();
            c_struct.user_list = vec_to_slurm_list(self.user_list);

            c_struct
        }
    }
}

/// Wrapper owning heap-allocated Slurm user condition struct
struct UserQueryInfo {
    user: *mut slurmdb_user_cond_t,
}

impl UserQueryInfo {
    #[allow(clippy::too_many_arguments)]
    fn new(
        assoc_config: AssocConfig,
        def_acct_list: Option<Vec<String>>,
        def_wckey_list: Option<Vec<String>>,
        with_assocs: bool,
        with_coords: bool,
        with_deleted: bool,
        with_wckey: bool,
        without_defaults: u16,
    ) -> Self {
        // build zeroed C struct and heap-allocate so Slurm destroy frees heap
        let mut c_user: slurmdb_user_cond_t = unsafe { std::mem::zeroed() };
        // assoc conditions
        let assoc_c = assoc_config.into_c_struct();
        c_user.assoc_cond = Box::into_raw(Box::new(assoc_c));
        c_user.admin_level = 0;
        c_user.def_acct_list = unsafe { vec_to_slurm_list(def_acct_list) };
        c_user.def_wckey_list = unsafe { vec_to_slurm_list(def_wckey_list) };
        c_user.with_assocs = bool_to_int(with_assocs);
        c_user.with_coords = bool_to_int(with_coords);
        c_user.with_deleted = bool_to_int(with_deleted);
        c_user.with_wckeys = bool_to_int(with_wckey);
        c_user.without_defaults = without_defaults;
        // heap allocate the user cond struct
        let boxed = Box::new(c_user);
        let ptr = Box::into_raw(boxed);
        Self { user: ptr }
    }
}

impl Drop for UserQueryInfo {
    fn drop(&mut self) {
        if !self.user.is_null() {
            unsafe {
                // Deconstruct the heap-allocated user condition
                let cond: &mut slurmdb_user_cond_t = &mut *self.user;
                // Destroy any Slurm lists in the struct
                if !cond.def_acct_list.is_null() {
                    slurm_list_destroy(cond.def_acct_list);
                }
                if !cond.def_wckey_list.is_null() {
                    slurm_list_destroy(cond.def_wckey_list);
                }
                // Destroy nested assoc_cond list struct
                if !cond.assoc_cond.is_null() {
                    // assoc_cond is a *mut slurmdb_assoc_cond_t; free its lists first
                    let assoc: &mut slurmdb_assoc_cond_t = &mut *cond.assoc_cond;
                    if !assoc.acct_list.is_null() {
                        slurm_list_destroy(assoc.acct_list);
                    }
                    if !assoc.cluster_list.is_null() {
                        slurm_list_destroy(assoc.cluster_list);
                    }
                    if !assoc.def_qos_id_list.is_null() {
                        slurm_list_destroy(assoc.def_qos_id_list);
                    }
                    if !assoc.format_list.is_null() {
                        slurm_list_destroy(assoc.format_list);
                    }
                    if !assoc.id_list.is_null() {
                        slurm_list_destroy(assoc.id_list);
                    }
                    if !assoc.parent_acct_list.is_null() {
                        slurm_list_destroy(assoc.parent_acct_list);
                    }
                    if !assoc.partition_list.is_null() {
                        slurm_list_destroy(assoc.partition_list);
                    }
                    if !assoc.qos_list.is_null() {
                        slurm_list_destroy(assoc.qos_list);
                    }
                    if !assoc.user_list.is_null() {
                        slurm_list_destroy(assoc.user_list);
                    }
                    // Now free the assoc_cond struct itself
                    let _ = Box::from_raw(cond.assoc_cond);
                }
                // Finally, free the outer user_cond struct
                let _ = Box::from_raw(self.user);
            }
            self.user = std::ptr::null_mut();
        }
    }
}

impl Deref for UserQueryInfo {
    type Target = slurmdb_user_cond_t;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.user }
    }
}

impl DerefMut for UserQueryInfo {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.user }
    }
}

fn create_user_cond(usernames: Vec<String>, usage_start: DateTime<Utc>, usage_end: DateTime<Utc>) -> UserQueryInfo {
    
    let assoc = AssocConfig {
        acct_list: None, 
        cluster_list: None, 
        def_qos_id_list: None, 
        flags: 0, 
        format_list: None, 
        id_list: None, 
        parent_acct_list: None, 
        partition_list: None, 
        qos_list: None, 
        usage_end, 
        usage_start, 
        user_list: Some(usernames)
    };

    UserQueryInfo::new(
        assoc,
        None,
        None,
        true,
        false,
        false,
        false,
        0,
    )
}

struct SlurmIterator {
    ptr: *mut list_itr_t
}

impl SlurmIterator {
    fn new(list_ptr: *mut xlist) -> Self {
        if list_ptr.is_null() {
            return Self { ptr: std::ptr::null_mut() };
        }
        let iter_ptr = unsafe { slurm_list_iterator_create(list_ptr) };

        Self {
            ptr: iter_ptr
        }
    }
}

impl Drop for SlurmIterator {
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
            return None 
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

struct SlurmUserList {
    ptr: *mut xlist
}

impl SlurmUserList {
    fn new(db_conn: &mut DbConn, user_query: &mut UserQueryInfo) -> Self {
        unsafe {
            // user_query.user is a *mut slurmdb_user_cond_t
            let ptr = slurmdb_users_get(db_conn.as_mut_ptr(), user_query.user);
            Self { ptr }
        }
    }
}

impl Drop for SlurmUserList {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                slurm_list_destroy(self.ptr)
            }
            self.ptr = std::ptr::null_mut();
        }
    }
}

#[derive(Error, Debug)]
enum QosError {
    #[error("Assoc vector was empty")]
    EmptyAssocError,
    #[error("No users found")]
    SlurmUserError,
    #[error("Could not find a default association")]
    DefaultAssocError,
    #[error("Pointer to assoc_list is null")]
    AssocListNull,
    #[error("Pointer to qos_list is null")]
    QosListNull,
    #[error("Pointer to user_list is null")]
    UserListNull,
    #[error("Database connection failed. Please ensure that SlurmDB is present and slurm_init has been run")]
    DbConnError,
    #[error("List of QoS successfully retrieved but empty")]
    EmptyQosListError,
}

#[derive(Debug)]
struct SlurmAssoc {
    acct: String,
    user: String,
    qos: Vec<String>,
}

impl SlurmAssoc {
    fn from_c_rec(rec: *const slurmdb_assoc_rec_t) -> Result<Self, QosError> {
        unsafe {
            let acct = if (*rec).acct.is_null() {
                String::new() 
            } else { 
                CStr::from_ptr((*rec).acct).to_string_lossy().into_owned() 
            };

            let user = if (*rec).user.is_null() {
                String::new() 
            } else { 
                CStr::from_ptr((*rec).user).to_string_lossy().into_owned()  
            };

            let qos = if !(*rec).qos_list.is_null() {
                let iterator = SlurmIterator::new((*rec).qos_list);
                let qos: Vec<String> = iterator.map(|node_ptr| {
                    let qos_ptr = node_ptr as *const i8;
                    if qos_ptr.is_null() {
                        String::new()
                    } else {
                        CStr::from_ptr(qos_ptr).to_string_lossy().into_owned() 
                    }
                }).collect();
                Ok(qos)
            } else {
                Err(QosError::QosListNull)
            }?;

            Ok(Self { acct, user, qos })
        }
    }
}

#[derive(Debug)]
struct SlurmUser {
    name: String,
    default_acct: String,
    admin_level: u16,
    associations: Vec<SlurmAssoc>
}

impl SlurmUser {
    fn from_c_rec(rec: *const slurmdb_user_rec_t) -> Result<Self, QosError> {
        unsafe {

            let name = if (*rec).name.is_null() {
                String::new() 
            } else { 
                CStr::from_ptr((*rec).name).to_string_lossy().into_owned() 
            };

            let default_acct = if (*rec).default_acct.is_null() {
                String::new() 
            } else { 
                CStr::from_ptr((*rec).default_acct).to_string_lossy().into_owned() 
            };

            let associations = if !(*rec).assoc_list.is_null() {
                let iterator = SlurmIterator::new((*rec).assoc_list);
                let associations: Vec<SlurmAssoc> = iterator.filter_map(|node_ptr| {
                    let assoc_ptr = node_ptr as *const slurmdb_assoc_rec_t;
                    SlurmAssoc::from_c_rec(assoc_ptr).ok()
                }).collect();
                // downside of not returning any of the error values, but this does allow usto 
                // be more fault tolerant and proceed if there are at least some valid values

                Ok(associations)
            } else {
                Err(QosError::AssocListNull)
            }?;
            
            Ok(Self {
                name,
                default_acct,
                admin_level: (*rec).admin_level, // we read actual admin value from database
                // record, but don't let this be used for any purposes other than reading it. Is
                // there any way to enforce that at the type level?
                associations
            })
        }
    }
}

fn process_user_list(user_list: SlurmUserList) -> Result<Vec<SlurmUser>, QosError> {
    if user_list.ptr.is_null() {
        return Err(QosError::UserListNull)
    }

    let iterator = SlurmIterator::new(user_list.ptr);
    let results: Vec<SlurmUser> = iterator.filter_map(|node_ptr| {
        let user_rec_ptr = node_ptr as *const slurmdb_user_rec_t;
        SlurmUser::from_c_rec(user_rec_ptr).ok()
    }).collect();

    Ok(results)
}

struct QosConfig {
    name_list: Option<Vec<String>>,
    format_list: Option<Vec<String>>,
    id_list: Option<Vec<String>>,
    //...
    // refer to slurmdb_qos_cond_t in bindings for more fields
}

impl QosConfig {
    fn into_c_struct(self) -> slurmdb_qos_cond_t {
        unsafe {
            let mut c_struct: slurmdb_qos_cond_t = std::mem::zeroed();
            c_struct.name_list = vec_to_slurm_list(self.name_list);
            c_struct.format_list = vec_to_slurm_list(self.format_list);
            c_struct.id_list = vec_to_slurm_list(self.id_list);
            //...

            c_struct
        }
    }
}

/// Wrapper owning a heap-allocated Slurm QoS filter struct
struct QosQueryInfo {
    qos: *mut slurmdb_qos_cond_t,
}

impl QosQueryInfo {
    fn new(config: QosConfig) -> Self {
        // build zeroed C struct and heap-allocate so Slurm destroy frees heap
        let c_struct: slurmdb_qos_cond_t = config.into_c_struct();
        let boxed = Box::new(c_struct);
        let ptr = Box::into_raw(boxed);
        Self { qos: ptr }
    }
}

impl Drop for QosQueryInfo {
    fn drop(&mut self) {
        if !self.qos.is_null() {
            unsafe {
                // First, destroy the Slurm-allocated lists inside the struct
                let cond: &mut slurmdb_qos_cond_t = &mut *self.qos;

                if !cond.name_list.is_null() {
                    slurm_list_destroy(cond.name_list);
                }
                if !cond.format_list.is_null() {
                    slurm_list_destroy(cond.format_list);
                }
                if !cond.id_list.is_null() {
                    slurm_list_destroy(cond.id_list);
                }
                // add more lists here as we add them to the struct

                // Then, reconstruct the Box from the raw pointer. This gives
                // ownership back to Rust, which will correctly free the memory
                let _ = Box::from_raw(self.qos);
            }
            self.qos = std::ptr::null_mut();
        }
    }
}

impl Deref for QosQueryInfo {
    type Target = slurmdb_qos_cond_t;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.qos }
    }
}

struct SlurmQosList {
    ptr: *mut xlist,
}

impl SlurmQosList {
    fn new(db_conn: &mut DbConn, qos_query: &mut QosQueryInfo) -> Self {
        unsafe {
            // qos_query.qos is a *mut slurmdb_qos_cond_t
            let ptr = slurmdb_qos_get(db_conn.as_mut_ptr(), qos_query.qos);
            Self { ptr }
        }
    }
}

impl Drop for SlurmQosList {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                slurm_list_destroy(self.ptr);
            }
        }
    }
}

#[derive(Debug)]
struct SlurmQos {
    name: String,
    priority: u32,
    max_jobs_per_user: u32,
    //...
    // refer to slurmdb_qos_rec_t in bindings
}

impl SlurmQos {
    fn from_c_rec(rec: *const slurmdb_qos_rec_t) -> Self {
        unsafe {
            // guard against null name pointer
            let name = if (*rec).name.is_null() {
                String::new()
            } else {
                CStr::from_ptr((*rec).name).to_string_lossy().into_owned()
            };
            Self {
                name,
                priority: (*rec).priority,
                max_jobs_per_user: (*rec).max_jobs_pu,
            }
        }
    }
}

fn process_qos_list(qos_list: SlurmQosList) -> Result<Vec<SlurmQos>, QosError> {

    if qos_list.ptr.is_null() {
        return Err(QosError::QosListNull)
    }

    let iterator = SlurmIterator::new(qos_list.ptr);

    let results: Vec<SlurmQos> = iterator.map(|node_ptr| {
        // not even an unsafe cast!
        let qos_rec_ptr = node_ptr as *const slurmdb_qos_rec_t;
        SlurmQos::from_c_rec(qos_rec_ptr)
    }).collect();

    if !results.is_empty() {
        Ok(results)
    } else {
        Err(QosError::EmptyQosListError)
    }

}


fn get_user_info(user_query: &mut UserQueryInfo, persist_flags: &mut u16) -> Result<Vec<SlurmQos>, QosError> {

    let db_conn_result = unsafe {
        slurmdb_connect(persist_flags) // connecting and getting the null pointer as a value that
    };

    let mut db_conn = match db_conn_result {
        Ok(conn) => Ok(conn),
        Err(_) => Err(QosError::DbConnError),
    }?;

    // will automatically drop when it drops out of scope

    // make sure that C can take in the user info struct 
    
    let user_list = SlurmUserList::new(&mut db_conn, user_query);

    let users = process_user_list(user_list)?;

    // assuming we only get one user back
    let Some(user) = users.first() else {
        return Err(QosError::SlurmUserError);
    };
    
    // find the association that matches the user's default account
    let Some(target_assoc) = user.associations.iter().find(|assoc| assoc.acct == user.default_acct) else {
        eprintln!("Default account association not found for user.");
        return Err(QosError::DefaultAssocError);
    };
    
    println!("Found QoS for account '{}': {:?}", target_assoc.acct, target_assoc.qos);
    
    // query for qos details
    let qos_details: Result<Vec<SlurmQos>, QosError> = if !target_assoc.qos.is_empty() {

        // build the query, currently very sparse
        let qos_config = QosConfig {
            name_list: None, // despite being returned as strings, the target_assoc_qos are
            // numbers, we need to pass them into id instead
            format_list: None,
            id_list: Some(target_assoc.qos.clone()),
        };

        // create the wrapper for the query
        let mut qos_query = QosQueryInfo::new(qos_config);
        
        // create the wrapper for the list, calls slurmdb_qos_get internally 
        let qos_list = SlurmQosList::new(&mut db_conn, &mut qos_query);
        
        // process the resulting list and get details
        process_qos_list(qos_list)
    } else {
        // qos detail error
        Err(QosError::EmptyAssocError)
    };

    qos_details

        // at all points, wrap these raw return into Rust types with Drop impls that use the
        // equivalent slurmdb_destroy_db function
        // and at the very end of the function, the connection will drop out of scope and close
        // itself
}

pub fn print_user_info(name: String) {

    let now = Utc::now();
    let assoc_config = AssocConfig {
        acct_list: None,
        cluster_list: None,
        def_qos_id_list: None,
        flags: 0, // bitflags
        format_list: None,
        id_list: None,
        parent_acct_list: None,
        partition_list: None,
        qos_list: None,
        usage_end: now - Duration::weeks(5),
        usage_start: now,
        user_list: Some(vec![name]),
    };

    let mut user_query = UserQueryInfo::new(assoc_config, None, None, true, false, false, false, 0);

    let mut persist_flags: u16 = 0;

    let qos_details = get_user_info(&mut user_query, &mut persist_flags);

    println!("QoS Details: {:?}", qos_details);
}

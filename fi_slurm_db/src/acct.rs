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
use chrono::{DateTime, Utc};

use rust_bind::bindings::{list_itr_t, slurm_list_append, slurm_list_create, slurm_list_destroy, slurm_list_iterator_create, slurm_list_iterator_destroy, slurm_list_next, slurmdb_admin_level_t_SLURMDB_ADMIN_NONE, slurmdb_assoc_cond_t, slurmdb_assoc_rec_t, slurmdb_connection_close, slurmdb_connection_get, slurmdb_destroy_assoc_cond, slurmdb_destroy_qos_cond, slurmdb_destroy_user_cond, slurmdb_qos_cond_t, slurmdb_qos_get, slurmdb_qos_rec_t, slurmdb_user_cond_t, slurmdb_user_rec_t, slurmdb_users_get, xlist};

struct DbConn {
    ptr: *mut c_void,
}

impl DbConn {
    fn new(persist_flags: u16) -> Self {
        unsafe {
            let ptr = slurmdb_connection_get(persist_flags);
            Self {
                ptr
            }
        }
    }
    
    fn as_mut_ptr(&mut self) -> *mut c_void { self.ptr }
}

impl Drop for DbConn {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                slurmdb_connection_close(self.ptr);
            }
            self.ptr = std::ptr::null_mut();
        }
    }
}

unsafe fn slurmdb_connect(persist_flags: u16) -> DbConn {
    DbConn::new(persist_flags)
}

fn bool_to_int(b: bool) -> u16 {
    if b {
        1
    } else {
        0
    }
}

unsafe fn vec_to_slurm_list(data: Option<Vec<String>>) -> *mut list {
    // If the Option is None, we return a null pointer, which Slurm ignores
    let Some(vec) = data else {
        return std::ptr::null_mut();
    };

    // If the vector is not empty, create a Slurm list
    let slurm_list = unsafe { slurm_list_create(None) };
    for item in vec {
        let c_string = CString::new(item).unwrap();
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

struct UserQueryInfo {
    // Rust version of slurmdb_user_cond_t
    user: slurmdb_user_cond_t
}

impl UserQueryInfo {
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
        unsafe {    
            let mut user: slurmdb_user_cond_t = std::mem::zeroed();

            let mut assoc_c_struct: slurmdb_assoc_cond_t = assoc_config.into_c_struct();

            // we are passing control of this memory from Rust to C
            // WE ARE NO LONGER RESPONSIBLE FOR DROPPING IT
            user.assoc_cond = Box::into_raw(Box::new(assoc_c_struct));

            user.admin_level = slurmdb_admin_level_t_SLURMDB_ADMIN_NONE; // we are hardcoding this,
            // DO NOT let users pass their own admin level
            user.def_acct_list = vec_to_slurm_list(def_acct_list);
            user.def_wckey_list = vec_to_slurm_list(def_wckey_list);
            user.with_assocs = bool_to_int(with_assocs);
            user.with_coords = bool_to_int(with_coords);
            user.with_deleted = bool_to_int(with_deleted);
            user.with_wckeys = bool_to_int(with_wckey);
            user.without_defaults = without_defaults;

            Self {user}
        }
    }
}

impl Drop for UserQueryInfo {
    fn drop(&mut self) {
        unsafe {
            slurmdb_destroy_user_cond(&mut self.user);
        }
    }
}

impl Deref for UserQueryInfo {
    type Target = slurmdb_user_cond_t;

    fn deref(&self) -> &Self::Target {
        &self.user
    }
}

impl DerefMut for UserQueryInfo {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.user
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

    let user = UserQueryInfo::new(
        assoc,
        None,
        None,
        true,
        false,
        false,
        false,
        0,
    );
    
    user
}

struct SlurmIterator {
    list: *mut list_itr_t
}

impl SlurmIterator {
    fn new(list: *mut xlist) -> Self {
        unsafe {
            let list_iter = slurm_list_iterator_create(list);

            Self {
                list: list_iter
            }
        }

    }
}

impl Drop for SlurmIterator {
    fn drop(&mut self) {
        unsafe {
            slurm_list_iterator_destroy(self.list);
        }
    }
}

struct SlurmUserList {
    ptr: *mut xlist
}

impl SlurmUserList {
    fn new(db_conn: &mut DbConn, user_query: UserQueryInfo) -> Self {
        unsafe {
            let ptr = slurmdb_users_get(db_conn.as_mut_ptr(), user_query.user);
            Self {
                ptr
            }
        }
    }
}

impl Drop for SlurmUserList {
    fn drop(&mut self) {
        unsafe {
            slurm_list_destroy(self.ptr);
        }
    }
}

#[derive(Debug)]
struct SlurmAssoc {
    acct: String,
    user: String,
    qos: Vec<String>,
}

// TODO: get rid of naked destroy functions, move that stuff into the type system
impl SlurmAssoc {
    fn from_c_rec(rec: *const slurmdb_assoc_rec_t) -> Self {
        unsafe {
            let acct = CStr::from_ptr((*rec).acct).to_string_lossy().into_owned();
            let user = CStr::from_ptr((*rec).user).to_string_lossy().into_owned();
            
            // The QoS list is another nested C list
            let mut qos = Vec::new();
            if !(*rec).qos_list.is_null() {
                let iterator = slurm_list_iterator_create((*rec).qos_list);
                while let Some(qos_ptr) = slurm_list_next(iterator) {
                    let qos_name = CStr::from_ptr(qos_ptr as *const i8).to_string_lossy().into_owned();
                    qos.push(qos_name);
                }
                slurm_list_iterator_destroy(iterator);
            }
            Self { acct, user, qos }
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

// TODO: get rid of naked destroy functions
impl SlurmUser {
    fn from_c_rec(rec: *const slurmdb_user_rec_t) -> Self {
        unsafe {
            let name = CStr::from_ptr((*rec).name).to_string_lossy().into_owned();
            let default_acct = CStr::from_ptr((*rec).default_acct).to_string_lossy().into_owned();

            let mut associations = Vec::new();

            if !(*rec).assoc_list.is_null() {
                let iterator = slurm_list_iterator_create((*rec).assoc_list);
                while let Some(node_ptr) = slurm_list_next(iterator) {
                    associations.push(SlurmAssoc::from_c_rec(node_ptr as *const slurmdb_assoc_rec_t));
                }
                slurm_list_iterator_destroy(iterator);
            }
            
            Self {
                name,
                default_acct,
                admin_level: (*rec).admin_level, // we read actual admin value from database
                // record, but don't let this be used for any purposes other than reading it. Is
                // there any way to enforce that at the type level?
                associations
            }
        }
    }
}

fn process_user_list(user_list: SlurmUserList) -> Vec<SlurmUser> {
    if user_list.ptr.is_null() {
        return Vec::new(); // make a more expressive error
    }

    let mut results = Vec::new();

    unsafe {
        let iterator = SlurmIterator::new(user_list.ptr);

        while let Some(node_ptr) = slurm_list_next(iterator.list) {

            let user_rec_ptr = node_ptr as *const slurmdb_user_rec_t;
            results.push(SlurmUser::from_c_rec(user_rec_ptr));
        }
    }

    results
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

struct QosQueryInfo {
    qos: slurmdb_qos_cond_t,
}

impl QosQueryInfo {
    fn new(config: QosConfig) -> Self {
        Self {
            qos: config.into_c_struct(),
        }
    }
}

impl Drop for QosQueryInfo {
    fn drop(&mut self) {
        unsafe {
            slurmdb_destroy_qos_cond(&mut self.qos);
        }
    }
}

impl Deref for QosQueryInfo {
    type Target = slurmdb_qos_cond_t;
    fn deref(&self) -> &Self::Target {
        &self.qos
    }
}

struct SlurmQosList {
    ptr: *mut xlist,
}

impl SlurmQosList {
    fn new(db_conn: &mut DbConn, qos_query: &QosQueryInfo) -> Self {
        unsafe {
            let ptr = slurmdb_qos_get(db_conn.as_mut_ptr(), qos_query);
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
            let name = CStr::from_ptr((*rec).name).to_string_lossy().into_owned();
            Self {
                name,
                priority: (*rec).priority,
                max_jobs_per_user: (*rec).max_jobs_pu,
            }
        }
    }
}

fn process_qos_list(qos_list: SlurmQosList) -> Vec<SlurmQos> {

    if qos_list.ptr.is_null() {
        return Vec::new(); // should we be returning an error and Result instead
        // of a blank vector? if we return this, it instead fails in the get function
        //  with an error print
    }

    let mut results = Vec::new();

    unsafe {
        let iterator = SlurmIterator::new(qos_list.ptr);

        while let Some(node_ptr) = slurm_list_next(iterator.list) {
            let qos_rec_ptr = node_ptr as *const slurmdb_qos_rec_t;
            results.push(SlurmQos::from_c_rec(qos_rec_ptr));
        }
    }

    results
}


fn get_user_info(user_query: UserQueryInfo, persist_flags: u16) {

    let db_conn = unsafe {
        slurmdb_connect(persist_flags) // connecting and getting the null pointer as a value that
    };
    // will automatically drop when it drops out of scope

    // make sure that C can take in the user info struct 
    
    let user_list = SlurmUserList::new(&mut db_conn, user_query);

    let users = process_user_list(user_list);

    // assuming we only get one user back
    let Some(user) = users.get(0) else {
        eprintln!("User not found.");
        // TODO: more expressive Err print
        return;
    };
    
    // find the association that matches the user's default account
    let Some(target_assoc) = user.associations.iter().find(|assoc| assoc.acct == user.default_acct) else {
        eprintln!("Default account association not found for user.");
        return;
    };
    
    println!("Found QoS for account '{}': {:?}", target_assoc.acct, target_assoc.qos);
    
    // query for qos details
    if !target_assoc.qos.is_empty() {


        // build the query, currently very sparse
        let qos_config = QosConfig {
            name_list: Some(target_assoc.qos.clone()),
            format_list: None,
            id_list: None,
        };

        // create the wrapper for the query
        let qos_query = QosQueryInfo::new(qos_config);
        
        // create the wrapper for the list, calls slurmdb_qos_get internally 
        let qos_list = SlurmQosList::new(&mut db_conn, &qos_query);
        
        // process the resulting list and get details
        let qos_details = process_qos_list(qos_list);

        // let qos_query = QosQueryInfo::new(Some(target_assoc.qos.clone()));
        //
        // let qos_list_ptr = unsafe { slurmdb_qos_get(*db_conn, &qos_query) };
        //
        // let qos_details = process_qos_list(qos_list_ptr);
        
        println!("QoS Details: {:?}", qos_details);
    }

        // at all points, wrap these raw return into Rust types with Drop impls that use the
        // equivalent slurmdb_destroy_db function
        // and at the very end of the function, the connection will drop out of scope and close
        // itself
}



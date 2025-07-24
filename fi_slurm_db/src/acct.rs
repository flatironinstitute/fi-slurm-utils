// prototyping the ability to grab database information

// example, replicating sacctmgr show qos cca format=MaxTRESPU%20,GrpTRES%20
//
// first, get user name, likely from OS env
// second, query for the user's associated accounts
// third, for each of those accounts, 

use std::{
    ffi::{CStr, CString}, 
    ops::{Deref, DerefMut}, 
    os::raw::c_void
};
use chrono::{DateTime, Utc};

use rust_bind::bindings::{list_itr_t, slurm_list_append, slurm_list_create, slurm_list_destroy, slurm_list_iterator_create, slurm_list_iterator_destroy, slurm_list_next, slurmdb_admin_level_t_SLURMDB_ADMIN_NONE, slurmdb_assoc_cond_t, slurmdb_connection_close, slurmdb_connection_get, slurmdb_destroy_assoc_cond, slurmdb_destroy_user_cond, slurmdb_qos_get, slurmdb_user_cond_t, slurmdb_user_rec_t, slurmdb_users_get, xlist};

struct DbConn {
    ptr: *mut c_void, // wait, is this even a pointer?
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

struct UserQueryInfo {
    // Rust version of slurmdb_user_cond_t
    user: slurmdb_user_cond_t
}

impl UserQueryInfo {
    fn new(
        admin_level: u16,
        assoc_cond: AssocQueryInfo,
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
            user.admin_level = admin_level;
            user.assoc_cond = assoc_cond.assoc;
            user.def_acct_list = vec_to_slurm_list(def_acct_list);
            user.def_wckey_list = vec_to_slurm_list(def_wckey_list);
            user.with_assocs = bool_to_int(with_assocs);
            user.with_coords = bool_to_int(with_coords);
            user.with_deleted = bool_to_int(with_deleted);
            user.with_wckeys = bool_to_int(with_wckey);
            user.without_defaults;

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

struct AssocQueryInfo {
    // Rust wrapper for slurmdb_assoc_cond_t 
    assoc: slurmdb_assoc_cond_t
}

impl AssocQueryInfo {
    fn new(
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
    ) -> Self {

        unsafe {
            let mut assoc: slurmdb_assoc_cond_t = std::mem::zeroed();

            assoc.acct_list = vec_to_slurm_list(acct_list);
            assoc.cluster_list = vec_to_slurm_list(cluster_list);
            assoc.def_qos_id_list = vec_to_slurm_list(def_qos_id_list);
            assoc.flags = flags;
            assoc.format_list = vec_to_slurm_list(format_list);
            assoc.id_list = vec_to_slurm_list(id_list);
            assoc.parent_acct_list = vec_to_slurm_list(parent_acct_list);
            assoc.partition_list = vec_to_slurm_list(partition_list);
            assoc.qos_list = vec_to_slurm_list(qos_list);
            assoc.usage_end = usage_end;
            assoc.usage_start = usage_start;
            assoc.user_list = vec_to_slurm_list(user_list);

            Self {assoc}
        }
    }
}

impl Drop for AssocQueryInfo {
    fn drop(&mut self) {
        unsafe {
            slurmdb_destroy_assoc_cond(&mut self.assoc);
        }
    }
}

// review this
impl Deref for AssocQueryInfo {
    type Target = slurmdb_assoc_cond_t;

    fn deref(&self) -> &Self::Target {
        &self.assoc
    }
}

impl DerefMut for AssocQueryInfo {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.assoc
    }
}

fn create_user_cond(username: String, usage_start: DateTime<Utc>, usage_end: DateTime<Utc>) -> UserQueryInfo {
    let assoc = AssocQueryInfo::new(
        None, 
        None, 
        None, 
        0, 
        None, 
        None, 
        None, 
        None, 
        None, 
        usage_end, 
        usage_start, 
        username
    );

    let user = UserQueryInfo::new(
        slurmdb_admin_level_t_SLURMDB_ADMIN_NONE, // aka 1, not admin. What if this is
        assoc,
        None,
        None,
        true,
        false,
        false,
        false,
        0,
    );
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
            slurm_list_iterator_destroy(self);
        }
    }
}

#[derive(Debug)]
struct SlurmUser {
    name: String,
    default_acct: String,
    admin_level: u16,
}

impl SlurmUser {
    /// Creates a safe Rust struct from a raw C pointer.
    /// This function encapsulates the unsafe C-string conversions.
    fn from_c_rec(rec: *const slurmdb_user_rec_t) -> Self {
        unsafe {
            let name = CStr::from_ptr((*rec).name).to_string_lossy().into_owned();
            let default_acct = CStr::from_ptr((*rec).default_acct).to_string_lossy().into_owned();
            
            Self {
                name,
                default_acct,
                admin_level: (*rec).admin_level,
            }
        }
    }
}

fn process_user_list(user_list_ptr: *mut xlist) -> Vec<SlurmUser> {    
    
    if user_list_ptr.is_null() {
        return Vec::new();
    }

    let mut results = Vec::new();

    unsafe {
        let iterator = SlurmIterator::new(user_list_ptr);

        while let Some(node_ptr) = slurm_list_next(iterator.list) {

            let user_rec_ptr = node_ptr as *const slurmdb_user_rec_t;
            results.push(SlurmUser::from_c_rec(user_rec_ptr));
        }

        slurm_list_destroy(user_list_ptr); // wrap this in a Drop struct
    }

    results
}

fn get_user_info(user_query: UserQueryInfo, persist_flags: u16) {

    let db_conn = unsafe {
        slurmdb_connect(persist_flags); // connecting and getting the null pointer as a value that
    };
    // will automatically drop when it drops out of scope

    // make sure that C can take in the user info struct 
    let user_list = unsafe {
        slurmdb_users_get(db_conn, user_query.user)
    };

    let users = process_user_list(user_list);

    let user1 = users[0];

    let default_account = user1.default_acct;



    
    let assoc_list = ...;

    for assoc in assoc_list {
        // get account name
        // get qos names from qos_list field
        
        // use the qos to create a slurmdb_qos_cond_t
        // put the qos's in the name list 
        //

        let qos_cond = ...;
        let qos_info = unsafe {
            slurmdb_qos_get(db_conn, qos_cond)
        };

        // display the qos info as desired


        // at all points, wrap these raw return into Rust types with Drop impls that use the
        // equivalent slurmdb_destroy_db function
        // and at the very end of the function, the connection will drop out of scope and close
        // itself
    }

}


// unsafe extern "C" {
//     pub fn slurmdb_users_get(
//         db_conn: *mut ::std::os::raw::c_void,
//         user_cond: *mut slurmdb_user_cond_t,
//     ) -> *mut list_t;
// }

// pub struct slurmdb_user_rec {
//     pub admin_level: u16,
//     pub assoc_list: *mut list_t,
//     pub bf_usage: *mut slurmdb_bf_usage_t,
//     pub coord_accts: *mut list_t,
//     pub default_acct: *mut ::std::os::raw::c_char,
//     pub default_wckey: *mut ::std::os::raw::c_char,
//     pub flags: u32,
//     pub name: *mut ::std::os::raw::c_char,
//     pub old_name: *mut ::std::os::raw::c_char,
//     pub uid: u32,
//     pub wckey_list: *mut list_t,
// }








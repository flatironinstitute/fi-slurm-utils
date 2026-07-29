use chrono::{DateTime, Duration, Utc};
use std::{
    collections::HashMap,
    ffi::CStr,
    ops::{Deref, DerefMut},
};

use fi_slurm_sys::{
    slurm_list_destroy, slurmdb_assoc_cond_t, slurmdb_assoc_rec_t, slurmdb_user_cond_t,
    slurmdb_user_rec_t, slurmdb_users_get, xlist,
};

use fi_slurm::partitions::{Partition, get_partitions};
use fi_slurm::site;

use users::get_current_username;

use crate::db::{DbConn, slurmdb_connect};
use crate::qos::{QosConfig, QosError, QosQueryInfo, SlurmQos, SlurmQosList, process_qos_list};
use fi_slurm::list::{SlurmIterator, vec_to_slurm_list};

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
pub struct UserQueryInfo {
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
        c_user.with_assocs = u16::from(with_assocs);
        c_user.with_coords = u16::from(with_coords);
        c_user.with_deleted = u16::from(with_deleted);
        c_user.with_wckeys = u16::from(with_wckey);
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

fn create_user_cond(
    usernames: Vec<String>,
    usage_start: DateTime<Utc>,
    usage_end: DateTime<Utc>,
) -> UserQueryInfo {
    let assoc = AssocConfig {
        acct_list: None,
        cluster_list: site::cluster().clone().map(|s| vec![s]),
        def_qos_id_list: None,
        flags: 0,
        format_list: None,
        id_list: None,
        parent_acct_list: None,
        partition_list: None,
        qos_list: None,
        usage_end,
        usage_start,
        user_list: Some(usernames),
    };

    UserQueryInfo::new(assoc, None, None, true, false, false, false, 0)
}

struct SlurmUserList {
    ptr: *mut xlist,
}

impl SlurmUserList {
    fn new(db_conn: &mut DbConn, user_query: &mut UserQueryInfo) -> Self {
        unsafe {
            // user_query.user is a *mut slurmdb_user_cond_t
            let ptr = slurmdb_users_get(db_conn.as_mut_ptr(), user_query.user);
            Self { ptr }
        }
    }

    /// Walks the records; the borrow keeps the list alive for as long as the iterator
    fn iter(&self) -> SlurmIterator<'_> {
        unsafe { SlurmIterator::new(self.ptr) }
    }
}

impl Drop for SlurmUserList {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { slurm_list_destroy(self.ptr) }
            self.ptr = std::ptr::null_mut();
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
struct SlurmAssoc {
    acct: String,
    id: u32,
    _user: String,
    qos: Vec<String>,
    comment: String,
}

impl SlurmAssoc {
    fn from_c_rec(rec: *const slurmdb_assoc_rec_t) -> Result<Self, QosError> {
        unsafe {
            let acct = if (*rec).acct.is_null() {
                String::new()
            } else {
                CStr::from_ptr((*rec).acct).to_string_lossy().into_owned()
            };

            let id = (*rec).id;

            let _user = if (*rec).user.is_null() {
                String::new()
            } else {
                CStr::from_ptr((*rec).user).to_string_lossy().into_owned()
            };

            let qos = if !(*rec).qos_list.is_null() {
                let iterator = SlurmIterator::new((*rec).qos_list);
                let qos: Vec<String> = iterator
                    .map(|node_ptr| {
                        let qos_ptr = node_ptr as *const i8;
                        if qos_ptr.is_null() {
                            String::new()
                        } else {
                            CStr::from_ptr(qos_ptr).to_string_lossy().into_owned()
                        }
                    })
                    .collect();
                Ok(qos)
            } else {
                Err(QosError::QosListNull)
            }?;

            let comment = if (*rec).comment.is_null() {
                String::new()
            } else {
                CStr::from_ptr((*rec).comment)
                    .to_string_lossy()
                    .into_owned()
            };

            Ok(Self {
                acct,
                id,
                _user,
                qos,
                comment,
            })
        }
    }
}

// need to pull more information out of assoc_rec_t

#[derive(Debug)]
struct SlurmUser {
    _name: String,
    _default_acct: String,
    _admin_level: u16,
    associations: Vec<SlurmAssoc>,
}

impl SlurmUser {
    fn from_c_rec(rec: *const slurmdb_user_rec_t) -> Result<Self, QosError> {
        unsafe {
            let _name = if (*rec).name.is_null() {
                String::new()
            } else {
                CStr::from_ptr((*rec).name).to_string_lossy().into_owned()
            };

            let _default_acct = if (*rec).default_acct.is_null() {
                String::new()
            } else {
                CStr::from_ptr((*rec).default_acct)
                    .to_string_lossy()
                    .into_owned()
            };

            let associations = if !(*rec).assoc_list.is_null() {
                let iterator = SlurmIterator::new((*rec).assoc_list);
                let associations: Vec<SlurmAssoc> = iterator
                    .filter_map(|node_ptr| {
                        let assoc_ptr = node_ptr as *const slurmdb_assoc_rec_t;
                        SlurmAssoc::from_c_rec(assoc_ptr).ok()
                    })
                    .collect();
                // downside of not returning any of the error values, but this does allow usto
                // be more fault tolerant and proceed if there are at least some valid values

                Ok(associations)
            } else {
                Err(QosError::AssocListNull)
            }?;

            Ok(Self {
                _name,
                _default_acct,
                _admin_level: (*rec).admin_level, // we read actual admin value from database
                // record, but don't let this be used for any purposes other than reading it. Is
                // there any way to enforce that at the type level?
                associations,
            })
        }
    }
}

fn process_user_list(user_list: SlurmUserList) -> Result<Vec<SlurmUser>, QosError> {
    if user_list.ptr.is_null() {
        return Err(QosError::UserListNull);
    }

    let results: Vec<SlurmUser> = user_list
        .iter()
        .filter_map(|node_ptr| {
            let user_rec_ptr = node_ptr as *const slurmdb_user_rec_t;
            SlurmUser::from_c_rec(user_rec_ptr).ok()
        })
        .collect();

    Ok(results)
}

/// Fetches the named QOS records in one query. An empty list would ask Slurm for every QOS,
/// so it short-circuits instead.
fn get_qos(db_conn: &mut DbConn, names: Vec<String>) -> Result<Vec<SlurmQos>, QosError> {
    if names.is_empty() {
        return Ok(Vec::new());
    }

    let qos_config = QosConfig {
        name_list: Some(names),
        format_list: None,
        id_list: None,
    };

    // create the wrapper for the query
    let mut qos_query = QosQueryInfo::new(qos_config);

    // create the wrapper for the list, calls slurmdb_qos_get internally
    let qos_list = SlurmQosList::new(db_conn, &mut qos_query);

    // process the resulting list and get details
    process_qos_list(qos_list)
}

fn handle_connection(persist_flags: &mut u16) -> Result<DbConn, QosError> {
    let db_conn_result = slurmdb_connect(persist_flags);

    let db_conn = match db_conn_result {
        Ok(conn) => Ok(conn),
        Err(_) => Err(QosError::DbConnError),
    }?;

    Ok(db_conn)
}

/// The account of the user's first association, the partitions that account may submit to, and
/// the QOS records those partitions draw their limits from.
///
/// A user can hold several associations, but only the first is reported on: the rest are
/// typically stale accounts from a previous center.
pub fn get_user_info(
    user_query: &mut UserQueryInfo,
    persist_flags: &mut u16,
) -> Result<(String, Vec<Partition>, Vec<SlurmQos>), QosError> {
    // will automatically drop when it drops out of scope
    let mut db_conn = handle_connection(persist_flags)?;

    // make sure that C can take in the user info struct
    let user_list = SlurmUserList::new(&mut db_conn, user_query);

    let users = process_user_list(user_list)?;

    // assuming we only get one user back
    let Some(user) = users.first() else {
        return Err(QosError::SlurmUserError);
    };

    let acct = &user
        .associations
        .first()
        .ok_or(QosError::EmptyAssocError)?
        .acct;

    let partitions: Vec<Partition> = get_partitions()
        .map_err(QosError::PartitionLoadError)?
        .into_iter()
        .filter(|p| p.allows_account(acct))
        .collect();

    // several partitions can share a QOS, so ask for each name once
    let mut qos_names: Vec<String> = partitions.iter().filter_map(|p| p.qos.clone()).collect();
    qos_names.sort();
    qos_names.dedup();

    let qos = get_qos(&mut db_conn, qos_names)?;

    Ok((acct.to_string(), partitions, qos))

    // at all points, wrap these raw return into Rust types with Drop impls that use the
    // equivalent slurmdb_destroy_db function
    // and at the very end of the function, the connection will drop out of scope and close
    // itself
}

/// The user's account, and the limits applying to them in each partition they can submit to
pub fn get_tres_info(name: Option<String>) -> Result<(String, Vec<PartitionLimits>), String> {
    let name = name.unwrap_or_else(|| {
        get_current_username().unwrap_or_else(|| {
            eprintln!("Could not find user information: ensure that the running user is not deleted while the program is running");
            "".into()
        }).to_string_lossy().into_owned() // handle the rare None case
    });

    let now = Utc::now();
    let mut user_query = create_user_cond(vec![name.clone()], now - Duration::weeks(5), now);

    let mut persist_flags: u16 = 0;

    let (user_acct, partitions, qos) = get_user_info(&mut user_query, &mut persist_flags)
        .map_err(|e| format!("Error getting user info for \"{name}\": {e:?}"))?;

    let by_name: HashMap<&str, &SlurmQos> = qos.iter().map(|q| (q.name.as_str(), q)).collect();

    let limits = partitions
        .iter()
        .map(|p| {
            let record = p.qos.as_deref().and_then(|name| by_name.get(name).copied());
            PartitionLimits::new(&p.name, p.qos.clone(), record)
        })
        .collect();

    Ok((user_acct, limits))
}

/// The limits that apply to a user's jobs in one partition, taken from that partition's QOS.
/// `None` is no limit, as is a partition having no QOS of its own; a `Some(0)` limit permits
/// nothing and is not the same thing.
#[derive(Clone, Default)]
pub struct PartitionLimits {
    pub partition: String,
    /// The QOS these limits came from, which is not always named after the partition
    pub qos: Option<String>,
    pub max_jobs_per_user: Option<u32>,
    pub max_tres_per_user: Option<String>,
    pub max_tres_per_group: Option<String>,
}

impl PartitionLimits {
    /// `qos` is the name the partition declares, `record` the QOS itself where slurmdbd
    /// had one to return
    fn new(partition: &str, qos: Option<String>, record: Option<&SlurmQos>) -> Self {
        let partition = partition.to_string();

        match record {
            Some(record) => Self {
                partition,
                qos,
                max_jobs_per_user: unlimited_to_none(record.max_jobs_per_user),
                max_tres_per_user: record.max_tres_per_user.clone(),
                max_tres_per_group: record.max_tres_per_group.clone(),
            },
            None => Self {
                partition,
                qos,
                ..Default::default()
            },
        }
    }
}

/// Slurm spells an unset scalar limit INFINITE (0xffffffff) or NO_VAL (0xfffffffe)
fn unlimited_to_none(limit: u32) -> Option<u32> {
    if limit >= u32::MAX - 1 {
        None
    } else {
        Some(limit)
    }
}

pub struct TresMax {
    pub max_nodes: Option<u32>,
    pub max_cores: Option<u32>,
    pub max_memory: Option<u32>,
    pub max_gpus: Option<u32>,
}

impl TresMax {
    pub fn new(tres: String) -> Self {
        let mut init: TresMax = Self {
            max_nodes: None,
            max_cores: None,
            max_memory: None,
            max_gpus: None,
        };

        tres.split(',').for_each(|t| {
            if let Some((category, quantity)) = t.split_once('=') {
                match category {
                    "1" => init.max_cores = Some(quantity.parse::<u32>().unwrap_or(8675309)),
                    "2" => init.max_memory = Some(quantity.parse::<u32>().unwrap_or(8675309)),
                    "4" => init.max_nodes = Some(quantity.parse::<u32>().unwrap_or(8675309)),
                    "1001" => init.max_gpus = Some(quantity.parse::<u32>().unwrap_or(8675309)),
                    _ => (),
                };
                //format!(" {quantity} {unit}")
            }
        });

        init
    }
}

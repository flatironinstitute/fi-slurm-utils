The `fi_slurm` crate contains an API for querying the Slurm daemon from Rust. It is centered on a read-only pattern in two steps: requesting data from Slurm with pointers, and transferring data from C to Rust. We shall demonstrate with the example of the top-level `get_jobs()` API function.

We first call the `RawSlurmJobInfo` struct's `load()` method, which initializes a raw pointer to memory owned by the `job_info_msg_t` C struct. We pass this pointer into the slurm_load_jobs C function, which we are accessing via an unsafe Rust binding.

We then discriminate its return code to ensure that the memory was correctly created on the C side, and if so, wrap the returned pointer to that memory in our safe Rust struct.

```
    let mut job_info_msg_ptr: *mut job_info_msg_t = std::ptr::null_mut();

    ...

    let return_code = unsafe {
        slurm_load_jobs(update_time, &mut job_info_msg_ptr, show_flags)
    };

    if return_code == 0 && !job_info_msg_ptr.is_null() {
        // Success: wrap the raw pointer in our safe struct and return it.
        Ok(Self { ptr: job_info_msg_ptr })
    } else {
        // Failure: return an error. No struct is created, no memory is leaked
        Err("Failed to load job information from Slurm".to_string())
    }

```

We then chain into the `into_slurm_jobs` method, which will consume this object and return a `SlurmJobs` struct.


For each C struct in the contiguous array of C-owned job memory, we attempt to create a Rust `Job` struct from the raw C struct, copying the information with a maximum memory requirement of 2n. Since each job struct is quite small and we expect at most a few thousand of them, we consider this memory cost acceptable.

Assuming that these all succeed, we can construct our `SlurmJobs` struct, which is now composed entirely of owned, Rust-side memory. 

```
    let raw_jobs_slice = self.as_slice();

    let jobs_map = raw_jobs_slice
        .iter()
        .try_fold(HashMap::new(), |mut map, raw_job| {
            let safe_job = Job::from_raw_binding(raw_job)?;
            map.insert(safe_job.job_id, safe_job);
            Ok::<HashMap<u32, Job>, String>(map)
        })?;
        
    let (last_update, last_backfill) = unsafe {
        let msg = &*self.ptr;
        (time_t_to_datetime(msg.last_update), time_t_to_datetime(msg.last_backfill))
    };
    
    Ok(SlurmJobs {
        jobs: jobs_map,
        last_update,
        last_backfill,
    })
}

```

But we must still ensure that the memory created and managed by C is freed by C. 

When the `RawSlurmJobInfo` struct was consumed, we activated its Drop implementation, as follows:

```
fn drop(&mut self) {
    if !self.ptr.is_null() {
        // This unsafe block is necessary to call the FFI free function
        // We are confident it's safe because we're calling the paired `free`
        // function on a non-null pointer that we own
        unsafe {
            slurm_free_job_info_msg(self.ptr);
        }
    }
}
```

In addition to dropping the Rust-owned memory, which is just a shell around the pointer to C memory, we perform an unsafe Rust binding call to the free function associated with this C struct, and pass in its pointer: in this way, we leave the cleanup of that memory to C, without ever having to manually free that memory when using the API.

This pattern of including the equivalent free function in the Drop implementation of a Rust struct that encapsulates the C pointer is the most idiomatic and streamlined way to manage a read-only API like this: any further functionality developed in this crate should follow this pattern.




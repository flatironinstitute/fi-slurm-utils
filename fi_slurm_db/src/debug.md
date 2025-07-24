(lldb) settings set -- target.run-args  "--qos"
(lldb) run
Process 2238052 launched: '/mnt/home/nposner/fi-utils/target/debug/fi_node' (x86_64)
Process 2238052 stopped
* thread #1, name = 'fi_node', stop reason = signal SIGSEGV: address not mapped to object (fault address: 0x0)
    frame #0: 0x0000000000000000
error: memory read failed for 0x0
(lldb) exit
Quitting LLDB will kill one or more processes. Do you really want to proceed: [Y/n] n
(lldb) bt
* thread #1, name = 'fi_node', stop reason = signal SIGSEGV: address not mapped to object (fault address: 0x0)
  * frame #0: 0x0000000000000000
    frame #1: 0x0000155554d5fb94 libslurm.so.42`acct_storage_g_get_connection(conn_num=0, persist_conn_flags=<unavailable>, rollback=true, cluster_name=<unavailable>) at accounting_storage.c:384:10
    frame #2: 0x0000155554c52b1d libslurm.so.42`slurmdb_connection_get(persist_conn_flags=<unavailable>) at connection_functions.c:54:9
    frame #3: 0x0000555555af3a04 fi_node`fi_slurm_db::acct::DbConn::new::hcec8198fe62bfa5f(persist_flags=0x00007fffffff8936) at acct.rs:37:23
    frame #4: 0x0000555555af3aa6 fi_node`fi_slurm_db::acct::slurmdb_connect::h8ce668cbc75f81a9(persist_flags=0x00007fffffff8936) at acct.rs:64:5
    frame #5: 0x0000555555af5bb9 fi_node`fi_slurm_db::acct::get_user_info::h616934bc083b0b29(user_query=0x00007fffffff88f8, persist_flags=0x00007fffffff8936) at acct.rs:533:9
    frame #6: 0x0000555555af6533 fi_node`fi_slurm_db::acct::print_user_info::h778498bfbe71050d(name=String @ 0x00007fffffff8be8) at acct.rs:614:23
    frame #7: 0x0000555555a3e425 fi_node`fi_node::main::hacdeab170d110ec0 at main.rs:39:9
    frame #8: 0x0000555555ac5a22 fi_node`core::ops::function::FnOnce::call_once::h7ffe580887ec5aa3((null)=(fi_node`fi_node::main::hacdeab170d110ec0 at main.rs:27), (null)=<unavailable>) at function.rs:250:5
    frame #9: 0x0000555555ab78d5 fi_node`std::sys::backtrace::__rust_begin_short_backtrace::hfd28c627036897fb(f=(fi_node`fi_node::main::hacdeab170d110ec0 at main.rs:27)) at backtrace.rs:152:18
    frame #10: 0x0000555555a4a1e4 fi_node`std::rt::lang_start::_$u7b$$u7b$closure$u7d$$u7d$::h8eb678feae64fb18 at rt.rs:195:18
    frame #11: 0x000055555640c06d fi_node`std::rt::lang_start_internal::ha7ac2373302ba363 + 221
    frame #12: 0x0000555555a4a1ba fi_node`std::rt::lang_start::h9213e274a978d5e9(main=(fi_node`fi_node::main::hacdeab170d110ec0 at main.rs:27), argc=2, argv=0x00007fffffff9b08, sigpipe='\0') at rt.rs:194:17
    frame #13: 0x0000555555a4460e fi_node`main + 30
    frame #14: 0x000015555303a7e5 libc.so.6`__libc_start_main + 229
    frame #15: 0x00005555559dc9be fi_node`_start + 46


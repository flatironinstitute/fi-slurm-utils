//use rust_bind::bindings::time_t;
//
//use crate::{jobs::{self, RawSlurmJobInfo}, nodes::{self, RawSlurmNodeInfo}};
//
//
//
//struct ClusterState {
//    nodes: nodes::SlurmNodes,
//    jobs: jobs::SlurmJobs,
//    last_nodes_update_time: time_t,
//    last_jobs_update_time: time_t,
//}
//
//impl ClusterState {
//    fn new() -> Self {
//        let node_info_msg = RawSlurmNodeInfo::load(0)?.into_message();
//        let job_info_msg = RawSlurmJobInfo::load(0)?.into_message();
//
//        // retrieve update and backfill information, parse the node and job slices as normal, 
//        // return the ClusterState object
//
//
//    } 
//
//    fn update(&mut self) {
//        let incremental_node_message = RawSlurmNodeInfo::load(self.last_nodes_update_time)?.into_message();
//        let incremental_job_message = RawSlurmJobInfo::load(self.last_jobs_update_time)?.into_message();
//
//        // make an incremental call to nodes and jobs, 
//        // update the cluster state
//        // perhaps store changes elsewhere for historical comparison?
//    }
//} 

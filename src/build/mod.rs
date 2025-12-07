use daggy::{stable_dag::StableDag, NodeIndex, Walker};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
};
use tokio::sync::Notify;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Derivation {
    pub name: String,
    pub drv_path: String,
    pub outputs: Vec<String>,
    pub system: String,
    pub input_drvs: Vec<String>,
    pub status: BuildStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildJob {
    pub derivation: Derivation,
    pub status: BuildStatus,
    pub requested_by: HashSet<i64>, // workflow IDs that need this derivation
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: i64,
    pub repository: String,
    pub commit_sha: String,
    pub attribute_set: String, // e.g. "packages.x86_64-linux"
    pub status: WorkflowStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum BuildStatus {
    Queued, // Waiting for dependencies to complete
    Ready,  // Dependencies satisfied, waiting for worker slot
    Running,
    Success,
    Cached, // Already in cache, no build needed
    Failed,
    Timedout,
    Canceled,
}
impl BuildStatus {
    pub fn done(self) -> bool {
        self == BuildStatus::Success
            || self == BuildStatus::Cached
            || self == BuildStatus::Failed
            || self == BuildStatus::Timedout
            || self == BuildStatus::Canceled
    }
    pub fn error(self) -> bool {
        self == BuildStatus::Failed
            || self == BuildStatus::Timedout
            || self == BuildStatus::Canceled
    }
}

impl std::fmt::Display for BuildStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildStatus::Queued => write!(f, "queued"),
            BuildStatus::Ready => write!(f, "ready"),
            BuildStatus::Running => write!(f, "running"),
            BuildStatus::Success => write!(f, "success"),
            BuildStatus::Cached => write!(f, "cached"),
            BuildStatus::Failed => write!(f, "failed"),
            BuildStatus::Timedout => write!(f, "timed out"),
            BuildStatus::Canceled => write!(f, "canceled"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkflowStatus {
    Running,
    Completed,
    Failed,
    Canceled,
}

#[derive(Debug, Default)]
struct BuildQueueState {
    dag: StableDag<BuildJob, ()>,
    drv_to_node: HashMap<String, NodeIndex>,
    ready: Vec<NodeIndex>,
    pending_workflows: HashMap<i64, usize>, // workflow_id -> count of unfinished jobs
}
#[derive(Debug, Default)]
pub struct BuildQueue {
    state: Mutex<BuildQueueState>,
    ready_signal: Notify,
}

impl BuildQueueState {
    /// Add jobs for a workflow. Returns true if the workflow is already complete.
    fn add_jobs(&mut self, derivations: Vec<Derivation>, workflow_id: i64) -> bool {
        let mut roots = Vec::new();
        let mut new_jobs_count = 0;
        let mut duplicate_pending_jobs = 0;

        // Add nodes
        for d in &derivations {
            if let Some(idx) = self.drv_to_node.get(&d.drv_path) {
                // Duplicate job, just add the workflow to requested_by
                let job = self.dag.node_weight_mut(*idx).unwrap();
                job.requested_by.insert(workflow_id);

                // Count duplicate jobs that aren't done yet
                if !job.status.done() {
                    duplicate_pending_jobs += 1;
                }
                continue;
            }

            // New job
            new_jobs_count += 1;

            let ready = d.input_drvs.is_empty();
            let mut requested_by = HashSet::new();
            requested_by.insert(workflow_id);
            let idx = self.dag.add_node(BuildJob {
                derivation: d.clone(),
                status: if ready {
                    BuildStatus::Ready
                } else {
                    BuildStatus::Queued
                },
                requested_by,
            });
            if ready {
                roots.push(idx);
            }
            self.drv_to_node.insert(d.drv_path.clone(), idx);
        }

        // Track pending jobs for this workflow (new jobs + duplicate jobs that aren't done)
        let total_pending = new_jobs_count + duplicate_pending_jobs;
        if total_pending > 0 {
            *self.pending_workflows.entry(workflow_id).or_insert(0) += total_pending;
        }

        // Add edges
        for d in derivations {
            let to_idx = self.drv_to_node.get(&d.drv_path).unwrap();
            for dep in &d.input_drvs {
                let from_idx = self.drv_to_node.get(dep).unwrap();
                if self.dag.find_edge(*from_idx, *to_idx).is_none() {
                    self.dag.add_edge(*from_idx, *to_idx, ()).unwrap();
                }
            }
        }
        //self.dag.transitive_reduce(roots.clone());
        self.ready.extend(roots.into_iter());

        // Workflow is complete if there are no pending jobs
        total_pending == 0
    }
    fn update_status(&mut self, drv_path: &str, status: BuildStatus) -> Vec<i64> {
        let Some(&id) = self.drv_to_node.get(drv_path) else {
            return Vec::new();
        };

        if status.error() {
            self.propagate_error(id, status)
        } else if status.done() {
            self.propagate_success(id, status)
        } else {
            let job = self.dag.node_weight_mut(id).unwrap();
            job.status = status;
            Vec::new()
        }
    }
    fn propagate_error(&mut self, id: NodeIndex, status: BuildStatus) -> Vec<i64> {
        let mut completed_workflows = Vec::new();

        let job = self.dag.node_weight_mut(id).unwrap();
        job.status = status;

        // Decrement workflow counters for this job
        let requested_by = job.requested_by.clone();
        for workflow_id in requested_by {
            if let Some(count) = self.pending_workflows.get_mut(&workflow_id) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    completed_workflows.push(workflow_id);
                    self.pending_workflows.remove(&workflow_id);
                }
            }
        }

        let mut walker = self.dag.parents(id);
        while let Some((e, _)) = walker.walk_next(&self.dag) {
            self.dag.remove_edge(e);
        }
        let mut walker = self.dag.children(id);
        while let Some((_, n)) = walker.walk_next(&self.dag) {
            self.ready.push(n);
            let child_completed = self.propagate_error(n, BuildStatus::Canceled);
            completed_workflows.extend(child_completed);
        }

        completed_workflows
    }
    fn propagate_success(&mut self, id: NodeIndex, status: BuildStatus) -> Vec<i64> {
        let mut completed_workflows = Vec::new();

        let job = self.dag.node_weight_mut(id).unwrap();
        job.status = status;

        // Decrement workflow counters for this job
        let requested_by = job.requested_by.clone();
        for workflow_id in requested_by {
            if let Some(count) = self.pending_workflows.get_mut(&workflow_id) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    completed_workflows.push(workflow_id);
                    self.pending_workflows.remove(&workflow_id);
                }
            }
        }

        let mut walker = self.dag.children(id);
        while let Some((e, n)) = walker.walk_next(&self.dag) {
            self.dag.remove_edge(e);
            let nparents = self.dag.parents(n).iter(&self.dag).count();
            if nparents == 0 {
                let j = self.dag.node_weight_mut(n).unwrap();
                j.status = BuildStatus::Ready;
                self.ready.push(n);
            }
        }

        completed_workflows
    }
    fn clear_workflow(&mut self, workflow_id: i64) {
        for (d, i) in self.drv_to_node.clone().into_iter() {
            let empty = {
                let job = self.dag.node_weight_mut(i).unwrap();
                job.requested_by.remove(&workflow_id);
                job.requested_by.is_empty()
            };
            if empty {
                self.dag.remove_node(i);
                self.drv_to_node.remove(&d);
            }
        }
    }
}
impl BuildQueue {
    pub fn new() -> Self {
        BuildQueue::default()
    }

    /// Add a batch of derivations from a workflow
    /// Returns true if the workflow is already complete (all jobs are done)
    pub fn add_workflow(&self, derivations: Vec<Derivation>, workflow_id: i64) -> bool {
        let mut state = self.state.lock().unwrap();
        let is_complete = state.add_jobs(derivations, workflow_id);
        if !state.ready.is_empty() {
            self.ready_signal.notify_one();
        }
        is_complete
    }
    pub async fn wait_for_ready_jobs(&self) {
        self.ready_signal.notified().await;
    }

    /// Drain the ready queue
    pub fn drain_ready_jobs(&self) -> Vec<BuildJob> {
        let mut state = self.state.lock().unwrap();
        let mut ret_ready = Vec::new();
        std::mem::swap(&mut ret_ready, &mut state.ready);
        ret_ready
            .into_iter()
            .map(|i| state.dag.node_weight(i).unwrap().clone())
            .collect()
    }

    /// Mark a job as done
    /// Returns list of workflow IDs that just completed (all their jobs are done)
    pub fn update_status(&self, drv_path: &str, status: BuildStatus) -> Vec<i64> {
        let mut state = self.state.lock().unwrap();
        let completed_workflows = state.update_status(drv_path, status);
        if !state.ready.is_empty() {
            self.ready_signal.notify_one();
        }
        completed_workflows
    }

    /// Remove a workflow's jobs from the queue
    pub fn clear_workflow(&self, workflow_id: i64) {
        let mut state = self.state.lock().unwrap();
        state.clear_workflow(workflow_id);
    }

    /// Get all jobs for a workflow (for detailed reporting and dashboard display)
    pub fn get_workflow_jobs(&self, workflow_id: i64) -> Vec<BuildJob> {
        let state = self.state.lock().unwrap();
        state
            .dag
            .graph()
            .node_weights()
            .filter(|job| job.requested_by.contains(&workflow_id))
            .cloned()
            .collect()
    }

    pub fn get_jobs(&self) -> Vec<BuildJob> {
        let state = self.state.lock().unwrap();
        state.dag.graph().node_weights().cloned().collect()
    }
}

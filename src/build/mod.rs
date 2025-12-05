use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BuildStatus {
    Queued,
    Running,
    Success,
    Failed,
    Cached, // Already in attic cache
}

impl std::fmt::Display for BuildStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildStatus::Queued => write!(f, "queued"),
            BuildStatus::Running => write!(f, "running"),
            BuildStatus::Success => write!(f, "success"),
            BuildStatus::Failed => write!(f, "failed"),
            BuildStatus::Cached => write!(f, "cached"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkflowStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug)]
pub struct BuildQueue {
    jobs: HashMap<String, BuildJob>, // keyed by drv_path
    queue_order: VecDeque<String>,   // drv_paths in order
}

impl Default for BuildQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl BuildQueue {
    pub fn new() -> Self {
        Self {
            jobs: HashMap::new(),
            queue_order: VecDeque::new(),
        }
    }

    pub fn add_job(&mut self, derivation: Derivation, workflow_id: i64) {
        let drv_path = derivation.drv_path.clone();

        if let Some(job) = self.jobs.get_mut(&drv_path) {
            job.requested_by.insert(workflow_id);
        } else {
            let mut requested_by = HashSet::new();
            requested_by.insert(workflow_id);

            let job = BuildJob {
                derivation,
                status: BuildStatus::Queued,
                requested_by,
            };

            self.jobs.insert(drv_path.clone(), job);
            self.queue_order.push_back(drv_path);
        }
    }

    pub fn next_job(&mut self) -> Option<String> {
        while let Some(drv_path) = self.queue_order.front() {
            if let Some(job) = self.jobs.get(drv_path) {
                if job.status == BuildStatus::Queued {
                    return Some(drv_path.clone());
                }
            }
            self.queue_order.pop_front();
        }
        None
    }

    pub fn get_job(&self, drv_path: &str) -> Option<&BuildJob> {
        self.jobs.get(drv_path)
    }

    pub fn get_job_mut(&mut self, drv_path: &str) -> Option<&mut BuildJob> {
        self.jobs.get_mut(drv_path)
    }

    pub fn get_jobs(&self) -> impl Iterator<Item = &BuildJob> {
        self.jobs.values()
    }
}

-- Initial database schema for Icicle CI

-- Builds table: stores metadata about individual derivation builds
CREATE TABLE IF NOT EXISTS builds (
    drv_path TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    system TEXT NOT NULL,
    status TEXT NOT NULL,  -- Queued, Running, Success, Failed, Cached
    started_at INTEGER,
    finished_at INTEGER,
    error_message TEXT
);

CREATE INDEX IF NOT EXISTS idx_builds_status ON builds(status);

-- Build-Workflow association table: tracks which workflows requested which builds
CREATE TABLE IF NOT EXISTS build_workflows (
    drv_path TEXT NOT NULL,
    workflow_id TEXT NOT NULL,
    PRIMARY KEY (drv_path, workflow_id),
    FOREIGN KEY (drv_path) REFERENCES builds(drv_path)
);

-- Workflows table: stores workflow metadata
CREATE TABLE IF NOT EXISTS workflows (
    id TEXT PRIMARY KEY,
    repository TEXT NOT NULL,
    commit_sha TEXT NOT NULL,
    attribute_set TEXT NOT NULL,
    status TEXT NOT NULL,  -- Pending, Running, Completed, Failed
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_workflows_created ON workflows(created_at DESC);

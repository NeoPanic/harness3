CREATE TABLE IF NOT EXISTS board_tasks(
 id TEXT PRIMARY KEY,root_id TEXT NOT NULL,parent_id TEXT REFERENCES board_tasks(id),
 title TEXT NOT NULL,instructions TEXT NOT NULL,acceptance TEXT NOT NULL,
 state TEXT NOT NULL,depth INTEGER NOT NULL,route_state TEXT NOT NULL DEFAULT 'unassigned',
 preset_name TEXT,preset_version TEXT,route_reason TEXT,instance_id TEXT REFERENCES instances(id),
 run_id TEXT REFERENCES runs(id),source_run_id TEXT REFERENCES runs(id),result TEXT,error TEXT,
 created_at INTEGER NOT NULL,finished_at INTEGER
);
CREATE INDEX IF NOT EXISTS board_tasks_ready ON board_tasks(state,route_state,created_at);
CREATE INDEX IF NOT EXISTS board_tasks_root ON board_tasks(root_id,created_at);
CREATE TABLE IF NOT EXISTS board_agents(
 instance_id TEXT PRIMARY KEY REFERENCES instances(id),root_id TEXT NOT NULL REFERENCES board_tasks(id),
 task_id TEXT NOT NULL REFERENCES board_tasks(id),run_id TEXT NOT NULL UNIQUE REFERENCES runs(id),
 claimed_task_id TEXT REFERENCES board_tasks(id),processed INTEGER NOT NULL DEFAULT 0,
 created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS board_commands(
 scope TEXT NOT NULL,key TEXT NOT NULL,fingerprint TEXT NOT NULL,response TEXT NOT NULL,
 PRIMARY KEY(scope,key)
);

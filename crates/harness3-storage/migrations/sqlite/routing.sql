CREATE TABLE IF NOT EXISTS board_routing(
 task_id TEXT PRIMARY KEY REFERENCES board_tasks(id), attempts INTEGER NOT NULL,
 retry_at INTEGER NOT NULL, state TEXT NOT NULL, error TEXT, model TEXT
);
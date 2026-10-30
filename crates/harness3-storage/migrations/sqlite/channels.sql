
CREATE TABLE IF NOT EXISTS channels(id TEXT PRIMARY KEY,goal_id TEXT NOT NULL REFERENCES goals(id),name TEXT NOT NULL,description TEXT NOT NULL,created_at INTEGER NOT NULL,UNIQUE(goal_id,name));
CREATE TABLE IF NOT EXISTS goal_team_channels(goal_id TEXT PRIMARY KEY REFERENCES goals(id),channel_id TEXT NOT NULL UNIQUE REFERENCES channels(id));
CREATE TABLE IF NOT EXISTS channel_members(channel_id TEXT NOT NULL REFERENCES channels(id),instance_id TEXT NOT NULL REFERENCES instances(id),subscribed INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(channel_id,instance_id));
CREATE TABLE IF NOT EXISTS channel_posts(seq INTEGER PRIMARY KEY AUTOINCREMENT,id TEXT NOT NULL UNIQUE,channel_id TEXT NOT NULL REFERENCES channels(id),author_kind TEXT NOT NULL,instance_id TEXT REFERENCES instances(id),source_run_id TEXT REFERENCES runs(id),text TEXT NOT NULL,wake_subscribers INTEGER NOT NULL,created_at INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS channel_posts_channel ON channel_posts(channel_id,seq);
CREATE TABLE IF NOT EXISTS channel_notifications(post_id TEXT NOT NULL REFERENCES channel_posts(id),recipient_id TEXT NOT NULL REFERENCES instances(id),state TEXT NOT NULL DEFAULT 'pending',message_id TEXT REFERENCES messages(id),error TEXT,PRIMARY KEY(post_id,recipient_id));

-- Durable state for the Codex app-server chat runtime.
--
-- Runtime data is intentionally separate from the legacy chat tables.  The
-- foreign keys make deleting a conversation remove only its runtime history;
-- the legacy message history remains independently readable for all other
-- conversations.
CREATE TABLE IF NOT EXISTS chat_runtime (
    conversation_id TEXT PRIMARY KEY REFERENCES conversations(id) ON DELETE CASCADE,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    run_id TEXT,
    phase TEXT NOT NULL DEFAULT 'idle',
    last_sequence INTEGER NOT NULL DEFAULT 0 CHECK (last_sequence >= 0),
    thread_id TEXT,
    turn_id TEXT,
    chat_turn INTEGER,
    message_id TEXT,
    last_client_request_id TEXT,
    last_steer_client_request_id TEXT,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS chat_runtime_events (
    conversation_id TEXT NOT NULL REFERENCES chat_runtime(conversation_id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    event_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (conversation_id, sequence)
);

CREATE TABLE IF NOT EXISTS chat_runtime_requests (
    conversation_id TEXT NOT NULL REFERENCES chat_runtime(conversation_id) ON DELETE CASCADE,
    request_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('command', 'file', 'question')),
    title TEXT NOT NULL,
    detail TEXT NOT NULL,
    questions_json TEXT NOT NULL DEFAULT '[]',
    server_method TEXT NOT NULL,
    server_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (conversation_id, request_id)
);

CREATE TABLE IF NOT EXISTS chat_runtime_replies (
    conversation_id TEXT NOT NULL REFERENCES chat_runtime(conversation_id) ON DELETE CASCADE,
    run_id TEXT NOT NULL,
    client_request_id TEXT NOT NULL,
    request_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (conversation_id, client_request_id)
);

-- Client request ids are durable operation keys.  A row in `pending` means
-- the process may have accepted the operation before the desktop disappeared;
-- callers must not replay it blindly after restart.
CREATE TABLE IF NOT EXISTS chat_runtime_operations (
    conversation_id TEXT NOT NULL REFERENCES chat_runtime(conversation_id) ON DELETE CASCADE,
    operation_kind TEXT NOT NULL CHECK (operation_kind IN ('start', 'reply', 'steer')),
    client_request_id TEXT NOT NULL,
    run_id TEXT,
    status TEXT NOT NULL CHECK (status IN ('pending', 'accepted', 'failed')),
    created_at TEXT NOT NULL,
    PRIMARY KEY (conversation_id, operation_kind, client_request_id)
);

CREATE INDEX IF NOT EXISTS idx_chat_runtime_events_conversation_sequence
    ON chat_runtime_events (conversation_id, sequence);
CREATE INDEX IF NOT EXISTS idx_chat_runtime_requests_conversation_created
    ON chat_runtime_requests (conversation_id, created_at, request_id);
CREATE INDEX IF NOT EXISTS idx_chat_runtime_operations_conversation_created
    ON chat_runtime_operations (conversation_id, created_at);
